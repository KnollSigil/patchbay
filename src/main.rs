use std::{cell::RefCell, collections::HashMap};

use pipewire::{context::ContextBox, loop_::Signal, main_loop::MainLoopRc, types::ObjectType};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mainloop = MainLoopRc::new(None)?;
    let context = ContextBox::new(&mainloop.loop_(), None)?;
    let core = context.connect(None)?;
    let registry = core.get_registry()?;

    // Nodes: want id, props, type_
    // Ports
    // Links
    // Device: maybe

    // Nodes <-> ports <-> links

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

    let nodes: RefCell<HashMap<u32, Node>> = RefCell::new(HashMap::new());
    let ports: RefCell<HashMap<u32, Port>> = RefCell::new(HashMap::new());
    let links: RefCell<HashMap<u32, Link>> = RefCell::new(HashMap::new());

    let _listener = registry
        .add_listener_local()
        .global(move |global| {
            match global.type_ {
                ObjectType::Node => {
                    println!("New node");
                    if let Some(props) = global.props {
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
                    println!("New port");
                    if let Some(props) = global.props {
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
                ObjectType::Link => {
                    println!("New link");
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
        .register();
    // let main_loop_weak = mainloop.downgrade();
    let loop_clone = mainloop.clone();
    let _sig_term = mainloop.loop_().add_signal_local(Signal::ALARM, move || {
        println!("Interrupted");
        // if let Some(main_loop) = main_loop_weak.upgrade() {
        //     main_loop.quit();
        // }
        loop_clone.quit();
    });

    mainloop.run();

    println!("Closed Gracefully");

    Ok(())
}