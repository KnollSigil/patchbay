use std::{cell::RefCell, collections::HashMap, rc::Rc, thread};

use pipewire::{context::{ContextBox, ContextRc}, loop_::Signal, main_loop::MainLoopRc, spa::param::ParamType, types::ObjectType};
use iced::widget::{Column, button, column, text};


struct Node {
    id: u32,
    // serial instead of id
    node_name: String,
    application_name: Option<String>,
    media_class: String,
}

struct Port {
    id: u32,
    port_id: String,
    name: String,
    direction: String,
    format_dsp: String,
    group: String,
    audio_channel: String,
}

struct Link {
    id: u32,
    input_port: u32,
    output_port: u32,
}

struct NodeGraph {
    nodes: HashMap<u32, Node>,
    ports: HashMap<u32, Port>,
    links: HashMap<u32, Link>,
}

enum Message {}
impl NodeGraph {
    fn view(&self) -> Column<'_, Message> {
        let mut column = Column::new();
        for (_, node) in &self.nodes {
            column = column.push(text(node.node_name.clone()));
        }
        column
    }
    fn update(&mut self, message: Message) {
        match message {
        }
    }
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self { nodes: Default::default(), ports: Default::default(), links: Default::default() }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pw_thread = thread::spawn(move || pw_thread().expect("Error running pipewire thread"));
    iced::application(NodeGraph::default, NodeGraph::update, NodeGraph::view)
    .run()?;

    pw_thread.join();

    println!("Closed Gracefully");

    Ok(())
}

fn pw_thread() -> Result<(), Box<dyn std::error::Error>> {
    let mainloop = MainLoopRc::new(None)?;
    let context = ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    let nodes: RefCell<HashMap<u32, Node>> = RefCell::new(HashMap::new());
    let ports: RefCell<HashMap<u32, Port>> = RefCell::new(HashMap::new());
    let links: RefCell<HashMap<u32, Link>> = RefCell::new(HashMap::new());

    let pending = Rc::new(RefCell::new(None));

    let core_weak = core.downgrade();
    let global_pending = Rc::downgrade(&pending);
    let _listener = registry
        .add_listener_local()
        .global(move |global| {
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
                            nodes.borrow_mut().insert(global.id, Node {
                                id: global.id,
                                node_name: node_name.unwrap(),
                                application_name: application_name,
                                media_class: media_class,
                            });
                        }
                    }
                }
                ObjectType::Port => {
                    if let Some(props) = global.props {
                        println!("New port {} {}", global.id, props.get("object.path").unwrap_or(""));
                        let mut port_id = None;
                        let mut name = None;
                        let mut direction = None;
                        let mut format_dsp = None;
                        let mut group = None;
                        let mut audio_channel = None;
                        for (key, val) in props.iter() {
                            match key {
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
                            ports.borrow_mut().insert(global.id, Port {
                                id: global.id,
                                port_id: port_id.unwrap(),
                                name: name.unwrap(),
                                direction: direction.unwrap(),
                                format_dsp: format_dsp.unwrap(),
                                group: group.unwrap(),
                                audio_channel,
                            });
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
                        links.borrow_mut().insert(global.id, Link {
                            id: global.id,
                            input_port: input_port.unwrap(),
                            output_port: output_port.unwrap(),
                        });
                    }
                }
                _ => {
                    println!("New global: {:#?}", global);
                }
            }
        })
        .global_remove({
            let global_pending = Rc::downgrade(&pending);
            let core_weak = core.downgrade();
            move |global| {
            println!("remove global: {}", global);
            let pending = global_pending.upgrade().unwrap();
            if *pending.borrow() == None {
                pending.replace(Some(core_weak.upgrade().unwrap().sync(0).expect("sync failed")));
                println!("New pending destroyed: {}", pending.borrow().unwrap().seq());
            }
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
    mainloop.run();
    Ok(())
}