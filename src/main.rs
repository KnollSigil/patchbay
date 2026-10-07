use std::{collections::{BTreeMap, BTreeSet, HashMap}};

use iced::{Element, Font, Length, futures::SinkExt, widget::{Row, button, row, scrollable}};
use iced_futures::core::font;
use iced::{Subscription, stream, widget::{Column, Text}};
use iced_aw::{TabBar, TabLabel, helpers::card, style};

mod pw;


struct NodeGraph {
    nodes: HashMap<u32, Node>,
    ports: HashMap<u32, pw::Port>,
    links: HashMap<u32, pw::Link>,
    pipewire_sender: Option<pipewire::channel::Sender<PwMessage>>,
    selected_node: Option<u32>,
    active_tab: NodeDetailsTabSelections,
}

struct Node {
    pw_node: pw::Node,
    pw_state: Option<pw::NodeState>,
    pw_props: Option<pw::NodeProps>,
}

#[derive(Clone)]
#[allow(unused)]
enum Message {
    PwSender(pipewire::channel::Sender<PwMessage>),
    PwUpdate(PwUpdate),
    UiAction(UiAction),
    Quit,
}

#[derive(Debug, Clone)]
enum UiAction {
    SelectedNode(u32),
    NodeDetailsTabAction(NodeDetailsTabAction),
    ToggleNodeConnection(u32, u32),
}

#[derive(Debug, Clone)]
enum NodeDetailsTabAction {
    TabSelected(NodeDetailsTabSelections),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NodeDetailsTabSelections {
    Links,
    Info,
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Self::PwSender(arg0) => f.debug_tuple("PwSender").field(arg0).finish(),
            Self::PwSender(_) => write!(f, "PwSender"),
            Self::PwUpdate(arg0) => f.debug_tuple("PwUpdate").field(arg0).finish(),
            Self::UiAction(arg0) => f.debug_tuple("UiAction").field(arg0).finish(),
            Self::Quit => write!(f, "Quit"),
        }
    }
}

#[derive(Debug, Clone)]
enum PwUpdate {
    AddNode(pw::Node),
    NodeInfo(pw::NodeInfo),
    AddPort(pw::Port),
    AddLink(pw::Link),
    Remove(u32),
}
impl NodeGraph {
    fn view(&self) -> Element<'_, Message> {
        let mut column = Column::new();

        let mut ports_for_nodes: HashMap<u32, Vec<u32>> = HashMap::new();
        for (_, port) in &self.ports {
            let port_list = ports_for_nodes.entry(port.node_id).or_default();
            port_list.push(port.id);
        }

        // TODO: sorting by global.id or port.id are not reliable
        for (_, ports) in &mut ports_for_nodes {
            ports.sort_by(|a, b| {
                let a = self.ports.get(a).unwrap();
                let b = self.ports.get(b).unwrap();
                a.port_id.cmp(&b.port_id)
            })
        }

        let mut links_for_ports: HashMap<u32, Vec<u32>> = HashMap::new();
        for (_, link) in &self.links {
            for link_port in [link.input_port, link.output_port] {
                let link_list = links_for_ports.entry(link_port).or_default();
                link_list.push(link.id);
            }
        }

        // TODO: loop in ordered fashion instead of just iterate through hashmap directly
        for (_, node) in &self.nodes {
            let mut port_column = Column::new();
            for port in ports_for_nodes.get(&node.pw_node.id).unwrap_or(&Vec::new()) {
                let port = self.ports.get(port).unwrap();
                port_column = port_column.push(Text::new(format!("{} {}", port.port_name, port.port_direction)))
            }

            let bold_font = Font {
                weight: font::Weight::Bold,
                ..Font::default()
            };

            
            let mut incoming_nodes = BTreeMap::new();
            let mut outgoing_nodes = BTreeMap::new();
            for port in ports_for_nodes.get(&node.pw_node.id).unwrap_or(&Vec::new()) {
                let port = self.ports.get(port).unwrap();
                for link in links_for_ports.get(&port.id).unwrap_or(&Vec::new()) {
                    let link = self.links.get(link).unwrap();
                    if port.port_direction.eq("in") {
                        let incoming_port = self.ports.get(&link.output_port).unwrap();
                        let links_for_incoming_node: &mut Vec<u32> = incoming_nodes.entry(incoming_port.node_id).or_default();
                        links_for_incoming_node.push(link.id);
                    }
                    if port.port_direction.eq("out") {
                        let outgoing_port = self.ports.get(&link.input_port).unwrap();
                        let links_for_outgoing_node: &mut Vec<u32> = outgoing_nodes.entry(outgoing_port.node_id).or_default();
                        links_for_outgoing_node.push(link.id);
                    }
                }
            }

            let mut input_link_column = Column::new();
            input_link_column = input_link_column.push(Text::new("Input Links").font(bold_font));

            for (incoming_node, links_for_incoming_node) in incoming_nodes {
                let mut incoming_node_row = Row::new();
                let incoming_node = self.nodes.get(&incoming_node).unwrap();
                incoming_node_row = incoming_node_row.push(Text::new(incoming_node.pw_node.node_name.clone()));

                let channel_spacing = 2.0;
                let channel_width = 5.0;
                let mut audio_table = Column::new().width(iced::Shrink).spacing(channel_spacing).padding(channel_spacing);

                for output_port in ports_for_nodes.get(&incoming_node.pw_node.id).unwrap() {
                    let output_port = self.ports.get(output_port).unwrap();
                    if !output_port.port_direction.eq("out") {
                        continue;
                    }
                    let mut connected_input_ports = Vec::new();
                    for link in &links_for_incoming_node {
                        let link = self.links.get(link).unwrap();
                        if link.output_port == output_port.id {
                            connected_input_ports.push(link.input_port);
                        }
                    }
                    let mut output_port_row = Row::new().spacing(channel_spacing);
                    for input_port in ports_for_nodes.get(&node.pw_node.id).unwrap() {
                        let input_port = self.ports.get(input_port).unwrap();
                        if !input_port.port_direction.eq("in") { continue; }
                        let color = if connected_input_ports.contains(&input_port.id) {
                            iced::color!(0, 255, 0)
                        } else {
                            iced::Color::WHITE
                        };
                        output_port_row = output_port_row.push(
                            iced::widget::container(iced::widget::space())
                                .width(Length::Fixed(channel_width))
                                .height(Length::Fixed(channel_width))
                                .style(move |_theme| iced::widget::container::background(iced::Background::Color(color)))
                        )
                    }
                    audio_table = audio_table.push(output_port_row);
                }

                let audio_container = iced::widget::Container::new(audio_table).style(|_theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(iced::Color::BLACK)),
                    ..iced::widget::container::Style::default()
                });
                incoming_node_row = incoming_node_row.push(audio_container);
                input_link_column = input_link_column.push(incoming_node_row);
            }

            let mut output_link_column = Column::new();
            output_link_column = output_link_column.push(Text::new("Output Links").font(bold_font));
            for (outgoing_node, links_for_outgoing_node) in outgoing_nodes {
                let mut outgoing_node_row = Row::new();
                let outgoing_node = self.nodes.get(&outgoing_node).unwrap();
                outgoing_node_row = outgoing_node_row.push(Text::new(outgoing_node.pw_node.node_name.clone()));
                
                let channel_spacing = 2.0;
                let channel_width = 5.0;
                let mut audio_table = Column::new().width(iced::Shrink).spacing(channel_spacing).padding(channel_spacing);

                for output_port in ports_for_nodes.get(&node.pw_node.id).unwrap() {
                    let output_port = self.ports.get(output_port).unwrap();
                    if !output_port.port_direction.eq("out") {
                        continue;
                    }
                    let mut connected_input_ports = Vec::new();
                    for link in &links_for_outgoing_node {
                        let link = self.links.get(link).unwrap();
                        if link.output_port == output_port.id {
                            connected_input_ports.push(link.input_port);
                        }
                    }
                    let mut output_port_row = Row::new().spacing(channel_spacing);
                    for input_port in ports_for_nodes.get(&outgoing_node.pw_node.id).unwrap() {
                        let input_port = self.ports.get(input_port).unwrap();
                        if !input_port.port_direction.eq("in") { continue; }
                        let color = if connected_input_ports.contains(&input_port.id) {
                            iced::color!(0, 255, 0)
                        } else {
                            iced::Color::WHITE
                        };
                        output_port_row = output_port_row.push(
                            iced::widget::container(iced::widget::space())
                                .width(Length::Fixed(channel_width))
                                .height(Length::Fixed(channel_width))
                                .style(move |_theme| iced::widget::container::background(iced::Background::Color(color)))
                        )
                    }
                    audio_table = audio_table.push(output_port_row);
                }

                let audio_container = iced::widget::Container::new(audio_table).style(|_theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(iced::Color::BLACK)),
                    ..iced::widget::container::Style::default()
                });
                outgoing_node_row = outgoing_node_row.push(audio_container);
                output_link_column = output_link_column.push(outgoing_node_row);
            }

            column = column.push(
                button(
                    card(
                    Text::new(node.pw_node.node_name.clone()),
                        row!(port_column, input_link_column, output_link_column).spacing(24),
                    ).style(style::card::primary)
                ).on_press(UiAction::SelectedNode(node.pw_node.id))
                .padding(0)
            );
        }
        let mut node_info_col = Column::new();
        if let Some(selected_node) = self.selected_node {
            let selected_node = self.nodes.get(&selected_node).unwrap();

            // pw::Node
            node_info_col = node_info_col.push(Text::new(format!("id: {}", selected_node.pw_node.id)));
            node_info_col = node_info_col.push(Text::new(format!("object_serial: {}", selected_node.pw_node.object_serial)));
            node_info_col = node_info_col.push(Text::new(format!("node.name: {}", selected_node.pw_node.node_name)));
            node_info_col = node_info_col.push(Text::new(format!("application.name: {:?}", selected_node.pw_node.application_name)));
            node_info_col = node_info_col.push(Text::new(format!("media.class: {:?}", selected_node.pw_node.media_class)));
            node_info_col = node_info_col.push(Text::new(format!("media.class: {:?}", selected_node.pw_node.media_type)));
            node_info_col = node_info_col.push(Text::new(format!("media.class: {:?}", selected_node.pw_node.media_category)));
            node_info_col = node_info_col.push(Text::new(format!("media.role: {:?}", selected_node.pw_node.media_role)));
            node_info_col = node_info_col.push(Text::new(format!("client.api: {:?}", selected_node.pw_node.client_api)));

            // pw::NodeState
            node_info_col = node_info_col.push(Text::new(format!("state: {:?}", selected_node.pw_state)));

            // pw::NodeProps
            if let Some(props) = &selected_node.pw_props {
                let mut sorted_props: Vec<(&String, &String)> = props.iter().collect();
                sorted_props.sort();
                for (prop_key, prop_value) in sorted_props {
                    node_info_col = node_info_col.push(Text::new(format!("prop {}: {}", prop_key, prop_value)));
                }
            }
        } else {
            node_info_col = node_info_col.push(Text::new("No selected node"));
        }

        let mut node_details_pane_tab_bar = TabBar::new(|tab| UiAction::NodeDetailsTabAction(NodeDetailsTabAction::TabSelected(tab)));
        node_details_pane_tab_bar = node_details_pane_tab_bar.push(NodeDetailsTabSelections::Links, TabLabel::Text("Links".to_string()));
        node_details_pane_tab_bar = node_details_pane_tab_bar.push(NodeDetailsTabSelections::Info, TabLabel::Text("Info".to_string()));
        node_details_pane_tab_bar = node_details_pane_tab_bar.set_active_tab(&self.active_tab);

        let node_details_pane_content: Element<'_, _> = match self.active_tab {
            NodeDetailsTabSelections::Links => self.view_node_link_pane(&ports_for_nodes, &links_for_ports),
            NodeDetailsTabSelections::Info => node_info_col.into(),
        };
        let mut node_details_pane = Column::new();
        node_details_pane = node_details_pane.push(node_details_pane_tab_bar);
        node_details_pane = node_details_pane.push(scrollable(node_details_pane_content).width(Length::Fill));
        let elem: Element<'_, UiAction> = row!(scrollable(column.spacing(8)).width(Length::Fill), node_details_pane.width(Length::Fill)).into();
        elem.map(Message::UiAction)
    }
    fn view_node_link_pane(&self, ports_for_nodes: &HashMap<u32, Vec<u32>>, links_for_ports: &HashMap<u32, Vec<u32>>) -> Element<'_, UiAction> {
        if let Some(selected_node) = self.selected_node {
            let mut input_nodes_col = Column::new().spacing(4);
            input_nodes_col = input_nodes_col.push(Text::new("Input Nodes"));

            let mut input_nodes = BTreeSet::new();
            for port in self.ports.values() {
                if port.port_direction.eq("out") {
                    input_nodes.insert(port.node_id);
                }
            }
            for input_node in input_nodes {
                let input_node = self.nodes.get(&input_node).unwrap();
                input_nodes_col = input_nodes_col.push(
                    button(input_node.pw_node.node_name.as_str())
                    .on_press(UiAction::ToggleNodeConnection(input_node.pw_node.id, selected_node))
                );
            }
            let mut output_nodes_col = Column::new().spacing(4);
            output_nodes_col = output_nodes_col.push(Text::new("Output Nodes"));
            row!(input_nodes_col.width(Length::Fill), output_nodes_col.width(Length::Fill)).into()
        } else {
            "No node selected".into()
        }
    }
    fn update(&mut self, message: Message) {
        // println!("received update message {:?}", message);
        match message {
            Message::PwSender(sender) => self.pipewire_sender = Some(sender),
            Message::PwUpdate(pw_update) => match pw_update {
                PwUpdate::AddNode(new_node) => {
                    self.nodes.insert(new_node.id, Node {
                        pw_node: new_node,
                        pw_state: None,
                        pw_props: None,
                    });
                }
                PwUpdate::NodeInfo(info) => {
                    if let Some(state) = info.state {
                        if let pw::NodeState::Error(error_string) = &state {
                            println!("Node error on {}: {}", info.id, error_string);
                        }
                        self.nodes.get_mut(&info.id).unwrap().pw_state = Some(state);
                    }
                    if let Some(props) = info.props {
                        self.nodes.get_mut(&info.id).unwrap().pw_props = Some(props);
                    }
                }
                PwUpdate::AddPort(new_port) => {
                    self.ports.insert(new_port.id, new_port);
                }
                PwUpdate::AddLink(new_link) => {
                    self.links.insert(new_link.id, new_link);
                }
                PwUpdate::Remove(removal_id) => {
                    self.nodes.remove(&removal_id);
                    self.ports.remove(&removal_id);
                    self.links.remove(&removal_id);
                    if let Some(selected_node) = self.selected_node && selected_node == removal_id {
                        self.selected_node = None;
                    }
                }
            },
            Message::UiAction(action) => match action {
                UiAction::SelectedNode(selected_node) => {
                    self.selected_node = Some(selected_node);
                }
                UiAction::NodeDetailsTabAction(action) => match action {
                    NodeDetailsTabAction::TabSelected(tab) => self.active_tab = tab,
                }
                UiAction::ToggleNodeConnection(outputting_node, inputting_node) => {
                    // TODO: deduplicate with index creation in view()
                    let mut ports_for_nodes: HashMap<u32, Vec<u32>> = HashMap::new();
                    for (_, port) in &self.ports {
                        let port_list = ports_for_nodes.entry(port.node_id).or_default();
                        port_list.push(port.id);
                    }

                    // TODO: sorting by global.id or port.id are not reliable
                    for (_, ports) in &mut ports_for_nodes {
                        ports.sort_by(|a, b| {
                            let a = self.ports.get(a).unwrap();
                            let b = self.ports.get(b).unwrap();
                            a.port_id.cmp(&b.port_id)
                        })
                    }
                    let mut current_connections = Vec::new();
                    for (_, link) in &self.links {
                        let link_input_port = self.ports.get(&link.input_port).unwrap();
                        let link_output_port = self.ports.get(&link.output_port).unwrap();
                        if link_input_port.node_id == inputting_node && link_output_port.node_id == outputting_node {
                            current_connections.push(PwRemovalId {
                                id: link.id,
                                object_serial: link.object_serial,
                            });
                        }
                    }
                    if current_connections.len() != 0 {
                        self.pipewire_sender.as_ref().unwrap().send(PwMessage::DestroyGlobals(current_connections)).expect("Failed to send RemoveLinks message to pipewire");
                    } else {
                        println!("No links from {} to {}", inputting_node, outputting_node); // auto-connect
                    }
                }
            }
            Message::Quit => {self.pipewire_sender.as_ref().unwrap().send(PwMessage::Terminate).expect("Failed to send message to pipewire");}
        }
    }
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self {
            nodes: Default::default(),
            ports: Default::default(),
            links: Default::default(),
            pipewire_sender: None,
            selected_node: None,
            active_tab: NodeDetailsTabSelections::Links,
        }
    }
}

#[derive(Debug)]
enum PwMessage {
    Terminate,
    DestroyGlobals(Vec<PwRemovalId>)
}

#[derive(Debug)]
struct PwRemovalId {
    id: u32,
    object_serial: u64,
}

struct QuitPwOnDrop {
    sender: pipewire::channel::Sender<PwMessage>,
}

impl Drop for QuitPwOnDrop {
    fn drop(&mut self) {
        self.sender.send(PwMessage::Terminate).expect("Failed to send message to pipewire");
    }
}

fn pipewire_subscription(_: &NodeGraph) -> Subscription<Message> {
    Subscription::run(|| stream::channel(100, async |mut output| {
        let (bridge_sender, mut bridge_receiver) = tokio::sync::mpsc::channel(256);
        let (pw_sender, pw_receiver) = pipewire::channel::channel();

        // iced will cancel this async stream, which will automatically drop our stack
        // when this variable gets dropped, it will tell our pipewire mainloop to terminate
        let _quit_on_drop = QuitPwOnDrop{sender: pw_sender.clone()};
        let result = output.send(Message::PwSender(pw_sender)).await;
        result.expect("Failed to send sender message");
        println!("sent sender message");

        let pw_thread = tokio::task::spawn_blocking(move || pw::run(bridge_sender, pw_receiver).expect("Error running pipewire thread"));
        println!("created pipewire thread");
        

        while let Some(ev) = bridge_receiver.recv().await {
            output.send(ev).await.expect("Failed to forward mpsc message");
        }

        drop(_quit_on_drop);
        println!("Done with subscription bridge loop");

        pw_thread.await.expect("Failed to wait for pipewire thread completion");
    }))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    iced::application(NodeGraph::default, NodeGraph::update, NodeGraph::view)
    .subscription(pipewire_subscription)
    .run()?;

    println!("Closed Gracefully");

    Ok(())
}
