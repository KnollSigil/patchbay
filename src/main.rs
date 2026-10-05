use std::collections::{BTreeMap, HashMap};

use iced::{Element, Font, Length, futures::SinkExt, widget::{Row, row, scrollable}};
use iced_futures::core::font;
use iced::{Subscription, stream, widget::{Column, Text}};
use iced_aw::{helpers::card, style};

mod pw;


struct NodeGraph {
    nodes: HashMap<u32, pw::Node>,
    ports: HashMap<u32, pw::Port>,
    links: HashMap<u32, pw::Link>,
    pipewire_sender: Option<pipewire::channel::Sender<PwMessage>>,
}

#[derive(Clone)]
enum Message {
    PwSender(pipewire::channel::Sender<PwMessage>),
    PwUpdate(PwUpdate),
    Quit,
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Self::PwSender(arg0) => f.debug_tuple("PwSender").field(arg0).finish(),
            Self::PwSender(_) => write!(f, "PwSender"),
            Self::PwUpdate(arg0) => f.debug_tuple("PwUpdate").field(arg0).finish(),
            Self::Quit => write!(f, "Quit"),
        }
    }
}

#[derive(Debug, Clone)]
enum PwUpdate {
    AddNode(pw::Node),
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

        for (_, mut ports) in &mut ports_for_nodes {
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
            for port in ports_for_nodes.get(&node.id).unwrap_or(&Vec::new()) {
                let port = self.ports.get(port).unwrap();
                port_column = port_column.push(Text::new(format!("{} {}", port.name, port.direction)))
            }

            let bold_font = Font {
                weight: font::Weight::Bold,
                ..Font::default()
            };

            
            let mut incoming_nodes = BTreeMap::new();
            let mut outgoing_nodes = BTreeMap::new();
            for port in ports_for_nodes.get(&node.id).unwrap_or(&Vec::new()) {
                let port = self.ports.get(port).unwrap();
                for link in links_for_ports.get(&port.id).unwrap_or(&Vec::new()) {
                    let link = self.links.get(link).unwrap();
                    if port.direction.eq("in") {
                        let incoming_port = self.ports.get(&link.output_port).unwrap();
                        let links_for_incoming_node: &mut Vec<u32> = incoming_nodes.entry(incoming_port.node_id).or_default();
                        links_for_incoming_node.push(link.id);
                    }
                    if port.direction.eq("out") {
                        let outgoing_port = self.ports.get(&link.input_port).unwrap();
                        let links_for_outgoing_node: &mut Vec<u32> = outgoing_nodes.entry(outgoing_port.node_id).or_default();
                        links_for_outgoing_node.push(link.id);
                    }
                }
            }

            let mut input_link_column = Column::new();
            input_link_column = input_link_column.push(Text::new("Input Links").font(bold_font));

            for (incoming_node, mut links_for_incoming_node) in incoming_nodes {
                let mut incoming_node_row = Row::new();
                let incoming_node = self.nodes.get(&incoming_node).unwrap();
                incoming_node_row = incoming_node_row.push(Text::new(incoming_node.node_name.clone()));

                let channel_spacing = 2.0;
                let channel_width = 5.0;
                let mut audio_table = Column::new().width(iced::Shrink).spacing(channel_spacing).padding(channel_spacing);

                for output_port in ports_for_nodes.get(&incoming_node.id).unwrap() {
                    let output_port = self.ports.get(output_port).unwrap();
                    if !output_port.direction.eq("out") {
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
                    for input_port in ports_for_nodes.get(&node.id).unwrap() {
                        let input_port = self.ports.get(input_port).unwrap();
                        if !input_port.direction.eq("in") { continue; }
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

                let audio_container = iced::widget::Container::new(audio_table).style(|theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(iced::Color::BLACK)),
                    ..iced::widget::container::Style::default()
                });
                incoming_node_row = incoming_node_row.push(audio_container);
                input_link_column = input_link_column.push(incoming_node_row);
            }

            let mut output_link_column = Column::new();
            output_link_column = output_link_column.push(Text::new("Output Links").font(bold_font));
            for (outgoing_node, mut links_for_outgoing_node) in outgoing_nodes {
                let mut outgoing_node_row = Row::new();
                let outgoing_node = self.nodes.get(&outgoing_node).unwrap();
                outgoing_node_row = outgoing_node_row.push(Text::new(outgoing_node.node_name.clone()));
                
                let channel_spacing = 2.0;
                let channel_width = 5.0;
                let mut audio_table = Column::new().width(iced::Shrink).spacing(channel_spacing).padding(channel_spacing);

                for output_port in ports_for_nodes.get(&node.id).unwrap() {
                    let output_port = self.ports.get(output_port).unwrap();
                    if !output_port.direction.eq("out") {
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
                    for input_port in ports_for_nodes.get(&outgoing_node.id).unwrap() {
                        let input_port = self.ports.get(input_port).unwrap();
                        if !input_port.direction.eq("in") { continue; }
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

                let audio_container = iced::widget::Container::new(audio_table).style(|theme| iced::widget::container::Style {
                    background: Some(iced::Background::Color(iced::Color::BLACK)),
                    ..iced::widget::container::Style::default()
                });
                outgoing_node_row = outgoing_node_row.push(audio_container);
                output_link_column = output_link_column.push(outgoing_node_row);
            }

            column = column.push(card(
                Text::new(node.node_name.clone()),
                    row!(port_column, input_link_column, output_link_column).spacing(24),
            )
            .style(style::card::primary));
        }
        scrollable(column.spacing(8)).into()
    }
    fn update(&mut self, message: Message) {
        println!("received update message {:?}", message);
        match message {
            Message::PwSender(sender) => self.pipewire_sender = Some(sender),
            Message::PwUpdate(pw_update) => match pw_update {
                PwUpdate::AddNode(new_node) => {
                    self.nodes.insert(new_node.id, new_node);
                }
                PwUpdate::AddPort(new_port) => {
                    println!("new port");
                    self.ports.insert(new_port.id, new_port);
                }
                PwUpdate::AddLink(new_link) => {
                    println!("new link");
                    self.links.insert(new_link.id, new_link);
                }
                PwUpdate::Remove(removal_id) => {
                    self.nodes.remove(&removal_id);
                    self.ports.remove(&removal_id);
                    self.links.remove(&removal_id);
                }
                _ => {},
            },
            Message::Quit => {self.pipewire_sender.as_ref().unwrap().send(PwMessage::Terminate).expect("Failed to send message to pipewire");}
            _ => ()
        }
    }
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self { nodes: Default::default(), ports: Default::default(), links: Default::default(), pipewire_sender: None }
    }
}

#[derive(Debug)]
enum PwMessage {
    Terminate,
}

fn pipewire_subscription(_: &NodeGraph) -> Subscription<Message> {
    Subscription::run(|| stream::channel(100, async |mut output| {
        let (bridge_sender, mut bridge_receiver) = tokio::sync::mpsc::channel(256);
        let (pw_sender, pw_receiver) = pipewire::channel::channel();

        let result = output.send(Message::PwSender(pw_sender)).await;
        result.expect("Failed to send sender message");
        println!("sent sender message");

        let pw_thread = tokio::task::spawn_blocking(move || pw::run(bridge_sender, pw_receiver).expect("Error running pipewire thread"));
        println!("created pipewire thread");
        

        while let Some(ev) = bridge_receiver.recv().await {
            output.send(ev).await.expect("Failed to forward mpsc message");
        }

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
