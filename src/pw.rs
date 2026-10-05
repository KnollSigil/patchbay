
use std::{cell::RefCell, rc::Rc};

use pipewire::{context::ContextRc, main_loop::MainLoopRc, types::ObjectType};
use crate::{Message, PwMessage, PwUpdate};

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Node {
    pub id: u32,
    // serial instead of id
    pub node_name: String,
    pub application_name: Option<String>,
    pub media_class: String,
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Port {
    pub id: u32,
    pub node_id: u32,
    pub port_id: String,
    pub name: String,
    pub direction: String,
    pub format_dsp: String,
    pub group: String,
    pub audio_channel: String,
}

#[derive(Debug, Clone)]
pub struct Link {
    pub id: u32,
    pub input_port: u32,
    pub output_port: u32,
}

pub fn run(main_sender:  tokio::sync::mpsc::Sender<Message>, pw_receiver: pipewire::channel::Receiver<PwMessage>) -> Result<(), Box<dyn std::error::Error>> {
    let mainloop = MainLoopRc::new(None)?;
    let context = ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

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
                                    audio_channel: audio_channel,
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
    let _attchrcv = pw_receiver.attach(mainloop.loop_(), {
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