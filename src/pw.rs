
use std::{cell::RefCell, collections::HashMap, rc::Rc};

use pipewire::{context::ContextRc, link, main_loop::MainLoopRc, node, proxy, types::ObjectType};
use crate::{Message, PwMessage, PwUpdate};

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Node {
    pub id: u32,
    // serial instead of id
    pub node_name: String,
    pub application_name: Option<String>,
    pub media_class: Option<String>,
    pub media_type: Option<String>,
    pub media_category: Option<String>,
    pub media_role: Option<String>,
    pub client_api: Option<String>,
    pub object_serial: u64,
}

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub id: u32,
    pub state: Option<NodeState>,
    pub props: Option<NodeProps>,
}

#[derive(Debug, Clone)]
pub enum NodeState {
    Error(String),
    Creating,
    Suspended,
    Idle,
    Running,
}
pub type NodeProps = HashMap<String, String>;

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Port {
    pub id: u32,
    pub object_serial: u64,
    pub object_path: String,
    pub node_id: u32,
    pub format_dsp: Option<String>,
    pub audio_channel: Option<String>,
    pub port_id: String,
    pub port_name: String,
    pub port_direction: String,
    pub port_alias: Option<String>,
    pub port_group: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Link {
    pub id: u32,
    pub input_port: u32,
    pub output_port: u32,
    pub object_serial: u64,
}

pub fn run(main_sender:  tokio::sync::mpsc::Sender<Message>, pw_receiver: pipewire::channel::Receiver<PwMessage>) -> Result<(), Box<dyn std::error::Error>> {
    let mainloop = MainLoopRc::new(None)?;
    let context = ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    let pending = Rc::new(RefCell::new(None));
    let main_sender = Rc::new(RefCell::new(main_sender));

    // proxy id -> proxy and listeners
    let proxy_listeners: Rc<RefCell<HashMap<u32, (Box<dyn proxy::ProxyT>, [Box<dyn proxy::Listener>; 2])>>> = Default::default();

    // global id -> object.serial
    let global_serial_map: Rc<RefCell<HashMap<u32, u64>>> = Default::default();

    let core_weak = core.downgrade();
    let global_pending = Rc::downgrade(&pending);
    let _listener = registry
        .add_listener_local()
        .global({
            let main_sender = main_sender.clone();
            let registry = registry.clone();
            let proxy_listeners = proxy_listeners.clone();
            let global_serial_map = global_serial_map.clone();
            move |global| {
                let pending = global_pending.upgrade().unwrap();
                if *pending.borrow() == None {
                    pending.replace(Some(core_weak.upgrade().unwrap().sync(0).expect("sync failed")));
                }
                let mut proxy_and_listener: Option<(Box<dyn proxy::ProxyT>, Box<dyn proxy::Listener>)> = None;
                let mut object_serial: Option<u64> = None;
                match global.type_ {
                    ObjectType::Node => {
                        if let Some(props) = global.props {
                            println!("New node {} {}", global.id, props.get("node.name").unwrap_or(""));
                            let mut node_name = None;
                            let mut media_class = None;
                            let mut media_type = None;
                            let mut media_category = None;
                            let mut application_name = None;
                            let mut media_role = None;
                            let mut client_api = None;
                            for (key, val) in props.iter() {
                                match key {
                                    "node.name" => node_name = Some(val.to_string()),
                                    "client.api" => client_api = Some(val.to_string()),
                                    "media.class" => media_class = Some(val.to_string()),
                                    "media.type" => media_type = Some(val.to_string()),
                                    "media.category" => media_category = Some(val.to_string()),
                                    "media.role" => media_role = Some(val.to_string()),
                                    "application.name" => application_name = Some(val.to_string()),
                                    "object.serial" => object_serial = Some(val.parse().expect(&format!("Unable to parse object.serial: {}", val))),
                                    _ => {},
                                }
                            }
                            let node: node::Node = registry.bind(global).unwrap();
                            let node_name_copy = node_name.as_ref().unwrap().clone();
                            let node_listener = node.add_listener_local()
                                .info({
                                    let main_sender = main_sender.clone();
                                    move |info| {
                                        println!("INFO: {:#?} {}: {:?}", info.id(), node_name_copy, info.state());
                                        let mut state = None;
                                        if info.change_mask().contains(node::NodeChangeMask::STATE) {
                                            state = Some(match info.state() {
                                                node::NodeState::Error(str) => NodeState::Error(str.to_string()),
                                                node::NodeState::Creating => NodeState::Creating,
                                                node::NodeState::Suspended => NodeState::Suspended,
                                                node::NodeState::Idle => NodeState::Idle,
                                                node::NodeState::Running => NodeState::Running,
                                            })
                                        }
                                        
                                        let mut props = None;
                                        if info.change_mask().contains(node::NodeChangeMask::PROPS) {
                                            props = Some(info.props().unwrap().iter().map(|(key, value)| {
                                                (key.to_string(), value.to_string())
                                            }).collect());
                                        }

                                        main_sender.borrow_mut().blocking_send(Message::PwUpdate(PwUpdate::NodeInfo(
                                            NodeInfo {
                                                id: info.id(),
                                                state: state,
                                                props: props,
                                            }
                                        ))).expect("Failed to send node info message");
                                    }
                                })
                                // .param(param) // TODO
                                .register();
                            proxy_and_listener = Some((Box::new(node), Box::new(node_listener)));
                            main_sender.borrow_mut().blocking_send(Message::PwUpdate(PwUpdate::AddNode(Node {
                                id: global.id,
                                node_name: node_name.unwrap(),
                                application_name: application_name,
                                media_class: media_class,
                                media_type,
                                media_category,
                                media_role: media_role,
                                client_api: client_api,
                                object_serial: object_serial.unwrap(),
                            }))).expect("Failed to send message to main");
                        }
                    }
                    ObjectType::Port => {
                        if let Some(props) = global.props {
                            println!("New port {} {}", global.id, props.get("object.path").unwrap_or(""));
                            let mut node_id = None;
                            let mut object_path = None;
                            let mut format_dsp = None;
                            let mut port_id = None;
                            let mut name = None;
                            let mut direction = None;
                            let mut port_alias = None;
                            let mut group = None;
                            let mut audio_channel = None;
                            for (key, val) in props.iter() {
                                match key {
                                    "node.id" => node_id = Some(val.parse::<u32>().unwrap()),
                                    "object.serial" => object_serial = Some(val.parse::<u64>().expect("Unable to parse object serial")),
                                    "object.path" => object_path = Some(val.to_string()),
                                    "format.dsp" => format_dsp = Some(val.to_string()),
                                    "port.id" => port_id = Some(val.to_string()),
                                    "port.name" => name = Some(val.to_string()),
                                    "port.direction" => direction = Some(val.to_string()),
                                    "port.alias" => port_alias = Some(val.to_string()),
                                    "port.group" => group = Some(val.to_string()),
                                    "audio.channel" => audio_channel = Some(val.to_string()),
                                    _ => {},
                                }
                            }
                            main_sender.borrow_mut().blocking_send(Message::PwUpdate(PwUpdate::AddPort(Port {
                                id: global.id,
                                object_serial: object_serial.unwrap(),
                                object_path: object_path.unwrap(),
                                node_id: node_id.unwrap(),
                                port_id: port_id.unwrap(),
                                port_name: name.unwrap(),
                                port_direction: direction.unwrap(),
                                format_dsp: format_dsp,
                                port_group: group,
                                audio_channel: audio_channel,
                                port_alias: port_alias,
                            }))).expect("Failed to send message to main");
                        }
                    }
                    ObjectType::Link => {
                        println!("New link {}", global.id);
                        if let Some(props) = global.props {
                            let mut input_port: Option<u32> = None;
                            let mut output_port: Option<u32> = None;
                            for (key, val) in props.iter() {
                                match key {
                                    "link.input.port" => input_port = Some(val.parse().unwrap()),
                                    "link.output.port" => output_port = Some(val.parse().unwrap()),
                                    "object.serial" => object_serial = Some(val.parse().unwrap()),
                                    _ => {},
                                }
                            }
                            main_sender.borrow_mut().blocking_send(Message::PwUpdate(PwUpdate::AddLink(Link {
                                id: global.id,
                                input_port: input_port.unwrap(),
                                output_port: output_port.unwrap(),
                                object_serial: object_serial.unwrap(),
                            }))).expect("Failed to send message to main");
                        }
                    }
                    _ => {
                        // println!("New global: {:#?}", global);
                    }
                }
                if let Some(object_serial) = object_serial {
                    global_serial_map.borrow_mut().insert(global.id, object_serial);
                }
                if let Some((proxy, listener)) = proxy_and_listener {
                    let proxy_up = proxy.upcast_ref();
                    let proxy_id = proxy_up.id();
                    let remove_listener = proxy_up.add_listener_local()
                        .removed({
                            let proxy_listeners = proxy_listeners.clone();
                            move || {
                                proxy_listeners.borrow_mut().remove(&proxy_id);
                            }
                        })
                        .register();
                    proxy_listeners.borrow_mut().insert(proxy_id, (proxy, [listener, Box::new(remove_listener)]));
                }
            }
        })
        .global_remove({
            let global_pending = Rc::downgrade(&pending);
            let main_sender = main_sender.clone();
            let core_weak = core.downgrade();
            let global_serial_map = global_serial_map.clone();
            move |global| {
                println!("remove global: {}", global);
                global_serial_map.borrow_mut().remove(&global);
                let pending = global_pending.upgrade().unwrap();
                if *pending.borrow() == None {
                    pending.replace(Some(core_weak.upgrade().unwrap().sync(0).expect("sync failed")));
                    println!("New pending destroyed: {}", pending.borrow().unwrap().seq());
                }
                main_sender.borrow_mut().blocking_send(Message::PwUpdate(PwUpdate::Remove(global)))
                    .expect("Failed to send message to main");
            }
        })
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
    let _attchrcv = pw_receiver.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        let registry = registry.clone();
        let global_serial_map = global_serial_map.clone();
        move |action| match action {
            PwMessage::Terminate => mainloop.quit(),
            PwMessage::DestroyGlobals(removal_list) => {
                for removal in removal_list {
                    println!("Destroying... {}:{}", removal.id, removal.object_serial);
                    if let Some(object_serial) = global_serial_map.borrow().get(&removal.id) {
                        if *object_serial == removal.object_serial {
                            registry.destroy_global(removal.id).into_result().expect("Failed to destroy pipewire global");
                        } else {
                            println!("global {} object.serial {} does not match object.serial of global to delete {}",
                                removal.id, object_serial, removal.object_serial
                            );
                        }
                    } else {
                        println!("Unable to find object.serial for global {} in global_serial_map", removal.id);
                    }
                }
            }
        }
    });
    mainloop.run();
    println!("quitted pipewire main loop");
    Ok(())
}