use std::{cell::RefCell, collections::HashMap, rc::Rc};

use iced::{Element, Font, futures::SinkExt, widget::{Row, row, scrollable}};
use iced_futures::core::{Widget, font};
use pipewire::{context::ContextRc, main_loop::MainLoopRc, types::ObjectType};
use iced::{Subscription, stream, widget::{Column, column, button, text, Text}};
use iced_aw::{helpers::card, style};


#[derive(Debug, Clone)]
struct Node {
    id: u32,
    // serial instead of id
    node_name: String,
    application_name: Option<String>,
    media_class: String,
}

#[derive(Debug, Clone)]
struct Port {
    id: u32,
    node_id: u32,
    port_id: String,
    name: String,
    direction: String,
    format_dsp: String,
    group: String,
    audio_channel: String,
}

#[derive(Debug, Clone)]
struct Link {
    id: u32,
    input_port: u32,
    output_port: u32,
}

struct NodeGraph {
    nodes: HashMap<u32, Node>,
    ports: HashMap<u32, Port>,
    links: HashMap<u32, Link>,
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
    AddNode(Node),
    AddPort(Port),
    AddLink(Link),
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

            let mut input_link_column = Column::new();
            input_link_column = input_link_column.push(Text::new("Input Links").font(bold_font));
            for port in ports_for_nodes.get(&node.id).unwrap_or(&Vec::new()) {
                let port = self.ports.get(port).unwrap();
                if port.direction.eq("in") {
                    for link in links_for_ports.get(&port.id).unwrap_or(&Vec::new()) {
                        let link = self.links.get(link).unwrap();
                        let incoming_port = self.ports.get(&link.output_port).unwrap();
                        let incoming_node = self.nodes.get(&incoming_port.node_id).unwrap();
                        input_link_column = input_link_column.push(Text::new(
                            format!("{}: {}->{}", incoming_node.node_name, incoming_port.name, port.name)
                        ));
                    }
                }
            }

            let mut output_link_column = Column::new();
            output_link_column = output_link_column.push(Text::new("Output Links").font(bold_font));
            for port in ports_for_nodes.get(&node.id).unwrap_or(&Vec::new()) {
                let port = self.ports.get(port).unwrap();
                if port.direction.eq("out") {
                    for link in links_for_ports.get(&port.id).unwrap_or(&Vec::new()) {
                        let link = self.links.get(link).unwrap();
                        let outgoing_port = self.ports.get(&link.input_port).unwrap();
                        let outgoing_node = self.nodes.get(&outgoing_port.node_id).unwrap();
                        output_link_column = output_link_column.push(Text::new(
                            format!("{}: {}->{}", outgoing_node.node_name, port.name, outgoing_port.name)
                        ));
                    }
                }
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

        let pw_thread = tokio::task::spawn_blocking(move || pw_thread(bridge_sender, pw_receiver).expect("Error running pipewire thread"));
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

fn pw_thread(mut main_sender:  tokio::sync::mpsc::Sender<Message>, pw_receiver: pipewire::channel::Receiver<PwMessage>) -> Result<(), Box<dyn std::error::Error>> {
    let mainloop = MainLoopRc::new(None)?;
    let context = ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    let nodes: RefCell<HashMap<u32, Node>> = RefCell::new(HashMap::new());
    let ports: RefCell<HashMap<u32, Port>> = RefCell::new(HashMap::new());
    let links: RefCell<HashMap<u32, Link>> = RefCell::new(HashMap::new());

    let pending = Rc::new(RefCell::new(None));
    let main_sender = Rc::new(RefCell::new(main_sender));

    let core_weak = core.downgrade();
    let global_pending = Rc::downgrade(&pending);
    let _listener = registry
        .add_listener_local()
        .global({
            let main_sender = main_sender.clone();
            move |global| {
                let pending = global_pending.upgrade().unwrap();
                if *pending.borrow() == None {
                    pending.replace(Some(core_weak.upgrade().unwrap().sync(0).expect("sync failed")));
                }
                match global.type_ {
                    ObjectType::Node => {
                        if let Some(props) = global.props {
                            println!("New node {} {}", global.id, props.get("node.name").unwrap_or(""));
                            let mut node_name = None;
                            let mut media_class = None;
                            let mut application_name = None;
                            for (key, val) in props.iter() {
                                match key {
                                    "node.name" => node_name = Some(val.to_string()),
                                    "media.class" => media_class = Some(val.to_string()),
                                    "application.name" => application_name = Some(val.to_string()),
                                    _ => {},
                                }
                            }
                            if let Some(media_class) = media_class {
                                main_sender.borrow_mut().blocking_send(Message::PwUpdate(PwUpdate::AddNode(Node {
                                    id: global.id,
                                    node_name: node_name.unwrap(),
                                    application_name: application_name,
                                    media_class: media_class,
                                }))).expect("Failed to send message to main");
                            }
                        }
                    }
                    ObjectType::Port => {
                        if let Some(props) = global.props {
                            println!("New port {} {}", global.id, props.get("object.path").unwrap_or(""));
                            let mut node_id = None;
                            let mut port_id = None;
                            let mut name = None;
                            let mut direction = None;
                            let mut format_dsp = None;
                            let mut group = None;
                            let mut audio_channel = None;
                            for (key, val) in props.iter() {
                                match key {
                                    "node.id" => node_id = Some(val.parse::<u32>().unwrap()),
                                    "port.id" => port_id = Some(val.to_string()),
                                    "port.name" => name = Some(val.to_string()),
                                    "port.direction" => direction = Some(val.to_string()),
                                    "format.dsp" => format_dsp = Some(val.to_string()),
                                    "port.group" => group = Some(val.to_string()),
                                    "audio.channel" => audio_channel = Some(val.to_string()),
                                    _ => {},
                                }
                            }
                            if let Some(audio_channel) = audio_channel {
                                main_sender.borrow_mut().blocking_send(Message::PwUpdate(PwUpdate::AddPort(Port {
                                    id: global.id,
                                    node_id: node_id.unwrap(),
                                    port_id: port_id.unwrap(),
                                    name: name.unwrap(),
                                    direction: direction.unwrap(),
                                    format_dsp: format_dsp.unwrap(),
                                    group: group.unwrap(),
                                    audio_channel,
                                }))).expect("Failed to send message to main");
                            }
                        }
                    }

                    // register node -> push to pending_nodes
                    // register port -> edit pending_nodes
                    // 
                    ObjectType::Link => {
                        println!("New link {}", global.id);
                        if let Some(props) = global.props {
                            let mut input_port: Option<u32> = None;
                            let mut output_port: Option<u32> = None;
                            for (key, val) in props.iter() {
                                match key {
                                    "link.input.port" => input_port = Some(val.parse().unwrap()),
                                    "link.output.port" => output_port = Some(val.parse().unwrap()),
                                    _ => {},
                                }
                            }
                            main_sender.borrow_mut().blocking_send(Message::PwUpdate(PwUpdate::AddLink(Link {
                                id: global.id,
                                input_port: input_port.unwrap(),
                                output_port: output_port.unwrap(),
                            }))).expect("Failed to send message to main");
                        }
                    }
                    _ => {
                        // println!("New global: {:#?}", global);
                    }
                }
            }
        })
        .global_remove({
            let global_pending = Rc::downgrade(&pending);
            let main_sender = main_sender.clone();
            let core_weak = core.downgrade();
            move |global| {
            println!("remove global: {}", global);
            let pending = global_pending.upgrade().unwrap();
            if *pending.borrow() == None {
                pending.replace(Some(core_weak.upgrade().unwrap().sync(0).expect("sync failed")));
                println!("New pending destroyed: {}", pending.borrow().unwrap().seq());
            }
            main_sender.borrow_mut().blocking_send(Message::PwUpdate(PwUpdate::Remove(global)))
                .expect("Failed to send message to main");
        }})
        .register();

    let _listener_core = core
        .add_listener_local()
        .done({
            let pending = Rc::downgrade(&pending);
            move |id, seq| {
                let pending = pending.upgrade().unwrap();
                if id == pipewire::core::PW_ID_CORE && pending.borrow().is_some_and(|p| seq == p) {
                    println!("sync");
                    pending.replace(None);
                }
            }
        })
        .register();
    let attchrcv = pw_receiver.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        move |_| {
            println!("got a PwMessage message");
            mainloop.quit();
        }
    });
    mainloop.run();
    println!("quitted pipewire main loop");
    Ok(())
}