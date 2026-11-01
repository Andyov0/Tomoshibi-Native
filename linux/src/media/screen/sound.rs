//! The sound of a shared screen: everything the computer plays except this
//! app, captured through PipeWire and sent as the share's sound
//! (`SoundKind::ScreenSound`, 48 kHz stereo).
//!
//! What is excluded is the point. The call itself plays through the same
//! speakers, and a capture of the default output's monitor would send
//! everybody's voice back into the call a beat late; Windows captures with
//! process loopback that leaves its own process out
//! (windows/Tomoshibi/Interop/Loopback.cs) for that reason. PipeWire has no
//! capture that leaves a process out, and pipewire-pulse's monitor source
//! is the whole mix. So this captures each application's sound where it
//! leaves the application instead: a capture stream of its own, connected
//! to nothing by the session manager (no AUTOCONNECT), and a link from the
//! output ports of every playback stream (`media.class` Stream/Output/Audio)
//! into its inputs. PipeWire mixes the links that meet at an input port, so
//! what arrives is the mix of every other application at the volume each
//! is set to, before the output's own volume. Left out:
//!
//! - this process's streams: the call's playback, whether through
//!   PulseAudio or PipeWire directly. A node is this process's when its own
//!   properties say `application.process.id` is (pipewire-pulse copies a
//!   PulseAudio client's there), or when its client's do, or when its
//!   client's connection is (`pipewire.sec.pid`, which the PipeWire server
//!   takes from the socket and no client can claim falsely);
//! - the playback halves of loopbacks, filter chains and echo cancellers
//!   (any node in a `node.link-group`): they carry sound some application
//!   already played, which is either captured at that application or, for
//!   the call played into an echo canceller's sink, exactly what must not
//!   go back.
//!
//! None of that is in what the registry announces about a node: a node's
//! global carries a short list of keys (its name, class and client), and its
//! process and link group are only in its full properties, which arrive once
//! the node is bound. So every playback node and every client is bound, and
//! a node is linked only once it has described itself and its client is
//! known: the first rig run linked the test's own tone in the moment before
//! anything said whose it was, and kept it, since the announcement never
//! said.
//!
//! Where there is no PipeWire (PulseAudio alone) there is no way to leave
//! the call out short of a capture per sink input mixed here, and a share
//! goes without its sound rather than echoing the call: `start` fails and
//! the share says so (`ScreenShare::sends_sound`). Every application's
//! sound, not one window's: the portal names no application for a window,
//! and a browser sharing a screen sends the system's sound, not a window's.
//!
//! The capture runs on a thread of its own with its own PipeWire loop, and
//! pushes into whichever input the share hands it (`feed`), so that it can
//! start before the sound is published and be handed a new input without
//! stopping.

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::JoinHandle,
    time::Duration,
};

use pipewire as pw;
use pw::{
    client::{Client, ClientListener},
    link::Link,
    node::{Node, NodeListener},
    properties::properties,
    registry::GlobalObject,
    spa::{
        self,
        param::{
            audio::{AudioFormat, AudioInfoRaw},
            ParamType,
        },
        pod::{serialize::PodSerializer, Object, Pod, Value},
        utils::{dict::DictRef, SpaTypes},
    },
    stream::{StreamFlags, StreamRc, StreamState},
    types::ObjectType,
};
use tokio::sync::mpsc::UnboundedSender;
use tomoshibi_core::call::AudioInput;

use super::Event;
use crate::{app::log, media::MediaError};

/// Where a share's sound goes now: the published input, or nowhere while
/// it is being published or after it was taken down.
pub type SoundSink = Arc<Mutex<Option<Arc<AudioInput>>>>;

/// One of the capture's two inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    Left,
    Right,
}

/// A node, as far as choosing what to hear.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeInfo {
    pub class: String,
    /// The client that made it (`client.id`).
    pub client: Option<u32>,
    /// `application.process.id` among its full properties.
    pub process: Option<u32>,
    pub in_link_group: bool,
    /// Its full properties have arrived; until then nothing above but the
    /// class and the client is known.
    pub described: bool,
}

/// A client: a connection to PipeWire.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientInfo {
    /// The process at the other end of its socket (`pipewire.sec.pid`).
    pub connected_from: Option<u32>,
    /// The process it says it is (`application.process.id`); for
    /// pipewire-pulse's clients, the PulseAudio application's.
    pub process: Option<u32>,
}

/// A port as the registry describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortInfo {
    pub node: u32,
    pub output: bool,
    pub channel: Option<String>,
    pub monitor: bool,
}

/// One link: from (node, port) to (node, port).
pub type Wire = (u32, u32, u32, u32);

/// What to bind to learn more of a global.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bind {
    Node,
    Client,
}

/// What the registry has said, and which node is this capture's.
#[derive(Debug, Default)]
pub struct Graph {
    pub nodes: BTreeMap<u32, NodeInfo>,
    pub clients: BTreeMap<u32, ClientInfo>,
    pub ports: BTreeMap<u32, PortInfo>,
    pub ours: Option<u32>,
}

const PLAYBACK: &str = "Stream/Output/Audio";

fn number(props: &DictRef, key: &str) -> Option<u32> {
    props.get(key).and_then(|value| value.parse().ok())
}

/// Which of the capture's inputs a channel goes to. Front and back and side
/// pairs keep their side; a mono or centre channel goes to both; the
/// low-frequency channel is left out, as a stereo downmix leaves it. A
/// channel with no position (AUX0, AUX1, ...) goes by its place among the
/// node's ports, alternately left and right.
fn sides(channel: Option<&str>, place: usize) -> &'static [Side] {
    match channel {
        Some("FL" | "RL" | "SL" | "FLC" | "TFL" | "TRL" | "RLC") => &[Side::Left],
        Some("FR" | "RR" | "SR" | "FRC" | "TFR" | "TRR" | "RRC") => &[Side::Right],
        Some("MONO" | "FC" | "RC" | "TC" | "TFC" | "TRC") => &[Side::Left, Side::Right],
        Some("LFE" | "LFE2") => &[],
        _ if place.is_multiple_of(2) => &[Side::Left],
        _ => &[Side::Right],
    }
}

impl Graph {
    /// Whether a node's sound is the screen's: another application's
    /// playback, known to be another's.
    pub fn is_heard(&self, node: &NodeInfo, own_process: u32) -> bool {
        if node.class != PLAYBACK || !node.described || node.in_link_group {
            return false;
        }
        let client = match node.client {
            Some(id) => match self.clients.get(&id) {
                Some(client) => Some(client),
                // Not yet announced: whose it is cannot be told.
                None => return false,
            },
            None => None,
        };
        let own = Some(own_process);
        !(node.process == own
            || client.is_some_and(|client| client.process == own || client.connected_from == own))
    }

    /// Every link the capture should have now.
    pub fn wanted(&self, own_process: u32) -> BTreeSet<Wire> {
        let mut wanted = BTreeSet::new();
        let Some(ours) = self.ours else {
            return wanted;
        };
        let inputs: BTreeMap<Side, u32> = self
            .ports
            .iter()
            .filter(|(_, port)| port.node == ours && !port.output)
            .filter_map(|(id, port)| match port.channel.as_deref() {
                Some("FL") => Some((Side::Left, *id)),
                Some("FR") => Some((Side::Right, *id)),
                _ => None,
            })
            .collect();
        if inputs.is_empty() {
            return wanted;
        }
        for (node_id, node) in &self.nodes {
            if *node_id == ours || !self.is_heard(node, own_process) {
                continue;
            }
            let outputs = self
                .ports
                .iter()
                .filter(|(_, port)| port.node == *node_id && port.output && !port.monitor);
            for (place, (port_id, port)) in outputs.enumerate() {
                for side in sides(port.channel.as_deref(), place) {
                    if let Some(input) = inputs.get(side) {
                        wanted.insert((*node_id, *port_id, ours, *input));
                    }
                }
            }
        }
        wanted
    }

    /// Takes in a global the registry announced; says what to bind to learn
    /// the rest of it.
    pub fn add(&mut self, id: u32, kind: &ObjectType, props: Option<&DictRef>) -> Option<Bind> {
        let props = props?;
        match kind {
            ObjectType::Node => {
                let class = props.get("media.class").unwrap_or_default().to_owned();
                let playback = class == PLAYBACK;
                self.nodes.insert(
                    id,
                    NodeInfo {
                        class,
                        client: number(props, "client.id"),
                        ..NodeInfo::default()
                    },
                );
                playback.then_some(Bind::Node)
            }
            ObjectType::Client => {
                self.clients.insert(
                    id,
                    ClientInfo {
                        connected_from: number(props, "pipewire.sec.pid"),
                        process: number(props, "application.process.id"),
                    },
                );
                Some(Bind::Client)
            }
            ObjectType::Port => {
                let node = number(props, "node.id")?;
                self.ports.insert(
                    id,
                    PortInfo {
                        node,
                        output: props.get("port.direction") == Some("out"),
                        channel: props.get("audio.channel").map(str::to_owned),
                        monitor: props.get("port.monitor") == Some("true"),
                    },
                );
                None
            }
            _ => None,
        }
    }

    /// A bound node's full properties.
    pub fn describe_node(&mut self, id: u32, props: Option<&DictRef>) {
        let Some(node) = self.nodes.get_mut(&id) else {
            return;
        };
        if let Some(props) = props {
            node.process = number(props, "application.process.id");
            node.in_link_group = props.get("node.link-group").is_some();
            node.client = number(props, "client.id").or(node.client);
        }
        node.described = true;
    }

    /// A bound client's full properties.
    pub fn describe_client(&mut self, id: u32, props: Option<&DictRef>) {
        let (Some(client), Some(props)) = (self.clients.get_mut(&id), props) else {
            return;
        };
        client.process = number(props, "application.process.id").or(client.process);
        client.connected_from = number(props, "pipewire.sec.pid").or(client.connected_from);
    }

    pub fn remove(&mut self, id: u32) {
        self.nodes.remove(&id);
        self.clients.remove(&id);
        self.ports.remove(&id);
        self.ports.retain(|_, port| port.node != id);
    }
}

/// The capture of the computer's sound for one share.
pub struct SoundCapture {
    asks: pw::channel::Sender<()>,
    thread: Option<JoinHandle<()>>,
    sink: SoundSink,
    chunks: Arc<AtomicU64>,
}

impl SoundCapture {
    /// Connects to the session's PipeWire and starts hearing every other
    /// application. Fails where there is no PipeWire to connect to.
    pub fn start(events: UnboundedSender<Event>) -> Result<SoundCapture, MediaError> {
        let sink: SoundSink = Arc::new(Mutex::new(None));
        let chunks = Arc::new(AtomicU64::new(0));
        let (asks, asked) = pw::channel::channel::<()>();
        let (started, starting) = mpsc::channel::<Result<(), String>>();
        let thread = std::thread::Builder::new()
            .name("tomoshibi-screen-sound".into())
            .spawn({
                let (sink, chunks) = (sink.clone(), chunks.clone());
                move || {
                    if let Err(fault) = run(sink, chunks, events, asked, &started) {
                        let _ = started.send(Err(fault));
                    }
                }
            })
            .map_err(|fault| MediaError::Failed(format!("no sound thread: {fault}")))?;
        match starting.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(SoundCapture {
                asks,
                thread: Some(thread),
                sink,
                chunks,
            }),
            Ok(Err(fault)) => Err(MediaError::Failed(format!("the computer's sound: {fault}"))),
            Err(_) => {
                let _ = asks.send(());
                Err(MediaError::Failed(
                    "the computer's sound did not start".into(),
                ))
            }
        }
    }

    /// Sends what is heard to `input` from now on, or nowhere.
    pub fn feed(&self, input: Option<Arc<AudioInput>>) {
        *self.sink.lock().unwrap_or_else(|e| e.into_inner()) = input;
    }

    /// How many pieces of sound have been pushed.
    pub fn chunks(&self) -> u64 {
        self.chunks.load(Ordering::Relaxed)
    }

    pub fn stop(mut self) -> Option<JoinHandle<()>> {
        self.feed(None);
        let _ = self.asks.send(());
        self.thread.take()
    }
}

impl Drop for SoundCapture {
    fn drop(&mut self) {
        let _ = self.asks.send(());
    }
}

/// The capture's state on its thread: the graph and the links made.
struct Linker {
    graph: Graph,
    links: BTreeMap<Wire, Link>,
    core: pw::core::CoreRc,
    own_process: u32,
}

impl Linker {
    /// Makes the links wanted that are not there, and lets go of those no
    /// longer wanted (their ports are gone, and the links with them).
    fn relink(&mut self) {
        let wanted = self.graph.wanted(self.own_process);
        self.links.retain(|wire, _| wanted.contains(wire));
        for wire in wanted {
            if self.links.contains_key(&wire) {
                continue;
            }
            let (out_node, out_port, in_node, in_port) = wire;
            let made = self.core.create_object::<Link>(
                "link-factory",
                &properties! {
                    "link.output.node" => out_node.to_string(),
                    "link.output.port" => out_port.to_string(),
                    "link.input.node" => in_node.to_string(),
                    "link.input.port" => in_port.to_string(),
                    // Gone with this capture: a link left behind would keep
                    // feeding a node that no longer exists into nothing.
                    "object.linger" => "false",
                },
            );
            match made {
                Ok(link) => {
                    log(&format!(
                        "screen sound: hearing node {out_node} port {out_port}"
                    ));
                    self.links.insert(wire, link);
                }
                Err(fault) => log(&format!(
                    "screen sound: no link from port {out_port}: {fault}"
                )),
            }
        }
    }
}

fn run(
    sink: SoundSink,
    chunks: Arc<AtomicU64>,
    events: UnboundedSender<Event>,
    asked: pw::channel::Receiver<()>,
    started: &mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    if !super::init_pipewire() {
        return Err("no PipeWire library".into());
    }
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|e| format!("main loop: {e}"))?;
    let context =
        pw::context::ContextRc::new(&mainloop, None).map_err(|e| format!("context: {e}"))?;
    let core = context
        .connect_rc(None)
        .map_err(|e| format!("no PipeWire: {e}"))?;
    let registry = core
        .get_registry_rc()
        .map_err(|e| format!("registry: {e}"))?;

    let linker = Rc::new(RefCell::new(Linker {
        graph: Graph::default(),
        links: BTreeMap::new(),
        core: core.clone(),
        own_process: std::process::id(),
    }));

    let stream = StreamRc::new(
        core.clone(),
        "tomoshibi-screen-sound",
        properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
            *pw::keys::NODE_NAME => "tomoshibi-screen-sound",
            *pw::keys::NODE_DESCRIPTION => "Tomoshibi: the shared screen's sound",
            // Linked here and nowhere else: the session manager would
            // otherwise connect a capture to the default microphone.
            *pw::keys::NODE_AUTOCONNECT => "false",
        },
    )
    .map_err(|e| format!("stream: {e}"))?;

    // The user data counts the frames sent, the lossless stream's clock.
    let _stream_listener = stream
        .add_local_listener_with_user_data(0u64)
        .state_changed({
            let (linker, events) = (linker.clone(), events.clone());
            move |stream, _, old, new| {
                log(&format!("screen sound stream: {old:?} -> {new:?}"));
                match new {
                    StreamState::Paused | StreamState::Streaming => {
                        let id = stream.node_id();
                        let mut linker = linker.borrow_mut();
                        if linker.graph.ours != Some(id) {
                            linker.graph.ours = Some(id);
                            linker.relink();
                        }
                    }
                    StreamState::Error(message) => {
                        let _ = events.send(Event::SoundGone(message));
                    }
                    _ => {}
                }
            }
        })
        .process(move |stream, sent| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let Some(data) = buffer.datas_mut().first_mut() else {
                return;
            };
            let (offset, size) = (data.chunk().offset() as usize, data.chunk().size() as usize);
            let Some(bytes) = data.data() else {
                return;
            };
            let Some(pcm) = bytes.get(offset..offset.saturating_add(size)) else {
                return;
            };
            if pcm.is_empty() {
                return;
            }
            let input = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(input) = input {
                input.push_pcm(pcm.to_vec());
                // The same samples for the lossless stream, which does
                // nothing for a sound not offered so (the screen's own, and
                // sound shared on its own with the switch off): the web and
                // the Mac send it beside sound shared on its own, and a
                // capture that only ever pushed the Opus track left every
                // listener with the lossy one. 16-bit samples are exact as
                // floats; the time is where they start on the stream's own
                // clock, which the sender checks for holes.
                let floats = pcm_floats(pcm);
                let frames = (floats.len() / 2) as u64;
                let at = (*sent * 1_000_000 / 48_000) as i64;
                *sent += frames;
                input.push_lossless(floats, 2, 48_000, at);
                chunks.fetch_add(1, Ordering::Relaxed);
            }
        })
        .register()
        .map_err(|e| format!("listener: {e}"))?;

    // The nodes and clients bound to read their full properties, each with
    // its listener, kept for as long as the global is there.
    let bound: Rc<RefCell<BTreeMap<u32, Bound>>> = Rc::default();
    let _registry_listener = registry
        .add_listener_local()
        .global({
            let (linker, bound, registry) = (linker.clone(), bound.clone(), registry.clone());
            move |global: &GlobalObject<&DictRef>| {
                let bind = linker
                    .borrow_mut()
                    .graph
                    .add(global.id, &global.type_, global.props);
                let made = match bind {
                    Some(Bind::Node) => bind_node(&registry, global, &linker),
                    Some(Bind::Client) => bind_client(&registry, global, &linker),
                    None => None,
                };
                if let Some(made) = made {
                    bound.borrow_mut().insert(global.id, made);
                }
                linker.borrow_mut().relink();
            }
        })
        .global_remove({
            let (linker, bound) = (linker.clone(), bound.clone());
            move |id| {
                bound.borrow_mut().remove(&id);
                let mut linker = linker.borrow_mut();
                linker.graph.remove(id);
                linker.relink();
            }
        })
        .register();

    let format = format_offer();
    let mut params = [Pod::from_bytes(&format).ok_or("the offered format")?];
    stream
        .connect(
            spa::utils::Direction::Input,
            None,
            StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .map_err(|e| format!("connect: {e}"))?;
    let _ = started.send(Ok(()));

    let _asked = asked.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        move |()| mainloop.quit()
    });
    mainloop.run();
    bound.borrow_mut().clear();
    linker.borrow_mut().links.clear();
    let _ = stream.disconnect();
    Ok(())
}

/// A node or client bound to hear its full properties.
enum Bound {
    Node(#[allow(dead_code)] Node, #[allow(dead_code)] NodeListener),
    Client(
        #[allow(dead_code)] Client,
        #[allow(dead_code)] ClientListener,
    ),
}

fn bind_node(
    registry: &pw::registry::RegistryRc,
    global: &GlobalObject<&DictRef>,
    linker: &Rc<RefCell<Linker>>,
) -> Option<Bound> {
    let node: Node = registry
        .bind(global)
        .map_err(|fault| {
            log(&format!(
                "screen sound: node {} not bound: {fault}",
                global.id
            ))
        })
        .ok()?;
    let (id, linker) = (global.id, linker.clone());
    let listener = node
        .add_listener_local()
        .info(move |info| {
            let mut linker = linker.borrow_mut();
            linker.graph.describe_node(id, info.props());
            linker.relink();
        })
        .register();
    Some(Bound::Node(node, listener))
}

fn bind_client(
    registry: &pw::registry::RegistryRc,
    global: &GlobalObject<&DictRef>,
    linker: &Rc<RefCell<Linker>>,
) -> Option<Bound> {
    let client: Client = registry
        .bind(global)
        .map_err(|fault| {
            log(&format!(
                "screen sound: client {} not bound: {fault}",
                global.id
            ))
        })
        .ok()?;
    let (id, linker) = (global.id, linker.clone());
    let listener = client
        .add_listener_local()
        .info(move |info| {
            let mut linker = linker.borrow_mut();
            linker.graph.describe_client(id, info.props());
            linker.relink();
        })
        .register();
    Some(Bound::Client(client, listener))
}

/// Little-endian 16-bit samples as floats from minus one to one.
pub fn pcm_floats(pcm: &[u8]) -> Vec<f32> {
    pcm.chunks_exact(2)
        .map(|pair| f32::from(i16::from_le_bytes([pair[0], pair[1]])) / 32768.0)
        .collect()
}

/// 16-bit stereo at 48 kHz, front left and right: what `AudioInput::push_pcm`
/// takes for a screen's sound, converted by PipeWire from whatever each
/// application plays.
fn format_offer() -> Vec<u8> {
    let mut info = AudioInfoRaw::new();
    info.set_format(AudioFormat::S16LE);
    info.set_rate(48_000);
    info.set_channels(2);
    let mut position = [0u32; spa::sys::SPA_AUDIO_MAX_CHANNELS as usize];
    position[0] = spa::sys::SPA_AUDIO_CHANNEL_FL;
    position[1] = spa::sys::SPA_AUDIO_CHANNEL_FR;
    info.set_position(position);
    let object = Object {
        type_: SpaTypes::ObjectParamFormat.as_raw(),
        id: ParamType::EnumFormat.as_raw(),
        properties: info.into(),
    };
    PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(object))
        .map(|(bytes, _)| bytes.into_inner())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: u32 = 4242;

    fn node(graph: &mut Graph, id: u32, class: &str, process: u32, in_link_group: bool) {
        graph.nodes.insert(
            id,
            NodeInfo {
                class: class.into(),
                client: None,
                process: Some(process),
                in_link_group,
                described: true,
            },
        );
    }

    fn port(graph: &mut Graph, id: u32, node: u32, output: bool, channel: &str) {
        graph.ports.insert(
            id,
            PortInfo {
                node,
                output,
                channel: Some(channel.into()),
                monitor: false,
            },
        );
    }

    /// A graph with the capture (node 1, inputs 10 and 11), another
    /// application playing stereo (node 2) and mono (node 3), this app's
    /// own playback (node 4), an echo canceller's playback half (node 5)
    /// and a sink with its monitor ports (node 6).
    fn graph() -> Graph {
        let mut graph = Graph {
            ours: Some(1),
            ..Graph::default()
        };
        node(&mut graph, 1, "Stream/Input/Audio", ME, false);
        port(&mut graph, 10, 1, false, "FL");
        port(&mut graph, 11, 1, false, "FR");
        node(&mut graph, 2, "Stream/Output/Audio", 100, false);
        port(&mut graph, 20, 2, true, "FL");
        port(&mut graph, 21, 2, true, "FR");
        node(&mut graph, 3, "Stream/Output/Audio", 101, false);
        port(&mut graph, 30, 3, true, "MONO");
        node(&mut graph, 4, "Stream/Output/Audio", ME, false);
        port(&mut graph, 40, 4, true, "FL");
        port(&mut graph, 41, 4, true, "FR");
        node(&mut graph, 5, "Stream/Output/Audio", 1, true);
        port(&mut graph, 50, 5, true, "FL");
        node(&mut graph, 6, "Audio/Sink", 1, false);
        port(&mut graph, 60, 6, false, "FL");
        graph.ports.insert(
            61,
            PortInfo {
                node: 6,
                output: true,
                channel: Some("FL".into()),
                monitor: true,
            },
        );
        graph
    }

    #[test]
    fn every_other_application_is_heard_and_this_one_is_not() {
        let wanted = graph().wanted(ME);
        assert_eq!(
            wanted,
            BTreeSet::from([
                (2, 20, 1, 10),
                (2, 21, 1, 11),
                (3, 30, 1, 10),
                (3, 30, 1, 11),
            ])
        );
    }

    #[test]
    fn nothing_is_linked_before_the_capture_has_its_ports() {
        let mut graph = graph();
        graph.ours = None;
        assert!(graph.wanted(ME).is_empty());
        let mut graph = self::graph();
        graph.ports.retain(|_, port| port.node != 1);
        assert!(graph.wanted(ME).is_empty());
    }

    #[test]
    fn a_surround_stream_keeps_its_sides_and_loses_its_bass() {
        let mut graph = graph();
        node(&mut graph, 7, "Stream/Output/Audio", 102, false);
        for (id, channel) in [
            (70, "FL"),
            (71, "FR"),
            (72, "FC"),
            (73, "LFE"),
            (74, "RL"),
            (75, "RR"),
        ] {
            port(&mut graph, id, 7, true, channel);
        }
        let from_seven: BTreeSet<(u32, u32)> = graph
            .wanted(ME)
            .into_iter()
            .filter(|wire| wire.0 == 7)
            .map(|wire| (wire.1, wire.3))
            .collect();
        assert_eq!(
            from_seven,
            BTreeSet::from([(70, 10), (71, 11), (72, 10), (72, 11), (74, 10), (75, 11)])
        );
    }

    #[test]
    fn a_node_gone_takes_its_ports_with_it() {
        let mut graph = graph();
        graph.remove(2);
        assert!(graph.wanted(ME).iter().all(|wire| wire.0 != 2));
        assert!(!graph.ports.contains_key(&20));
    }

    /// What the registry announces of a node (its class and client, not
    /// its process) and what binding it then says, as PipeWire 1.0 does: a
    /// node is heard only once both are known, and never when its client is
    /// this process, however it was announced.
    #[test]
    fn a_node_is_heard_only_once_it_is_known_to_be_another_process() {
        let mut graph = graph();
        let props = |pairs: &[(&str, &str)]| {
            let mut props = pw::properties::PropertiesBox::new();
            for (key, value) in pairs {
                props.insert(*key, *value);
            }
            props
        };
        let heard_from =
            |graph: &Graph, node: u32| graph.wanted(ME).iter().any(|wire| wire.0 == node);

        // This process's own tone through PipeWire directly: client 90,
        // connected from this process; the node says nothing of a process.
        let client = props(&[("pipewire.sec.pid", &ME.to_string())]);
        assert_eq!(
            graph.add(90, &ObjectType::Client, Some(client.as_ref())),
            Some(Bind::Client)
        );
        let announced = props(&[("media.class", "Stream/Output/Audio"), ("client.id", "90")]);
        assert_eq!(
            graph.add(9, &ObjectType::Node, Some(announced.as_ref())),
            Some(Bind::Node)
        );
        port(&mut graph, 95, 9, true, "FL");
        assert!(!heard_from(&graph, 9), "linked before it said whose it is");
        graph.describe_node(
            9,
            Some(props(&[("node.name", "own-tone"), ("client.id", "90")]).as_ref()),
        );
        assert!(
            !heard_from(&graph, 9),
            "this process's own playback was heard"
        );

        // Another application's, announced the same way, through a client
        // not announced yet.
        let announced = props(&[("media.class", "Stream/Output/Audio"), ("client.id", "91")]);
        graph.add(8, &ObjectType::Node, Some(announced.as_ref()));
        port(&mut graph, 85, 8, true, "FL");
        graph.describe_node(8, Some(props(&[("client.id", "91")]).as_ref()));
        assert!(!heard_from(&graph, 8), "heard before its client was known");
        graph.add(
            91,
            &ObjectType::Client,
            Some(props(&[("pipewire.sec.pid", "777")]).as_ref()),
        );
        assert!(heard_from(&graph, 8));

        // A PulseAudio application through pipewire-pulse: the connection is
        // pipewire-pulse's, and the process is in the client's properties.
        graph.add(
            92,
            &ObjectType::Client,
            Some(props(&[("pipewire.sec.pid", "555")]).as_ref()),
        );
        graph.describe_client(
            92,
            Some(props(&[("application.process.id", &ME.to_string())]).as_ref()),
        );
        graph.add(
            7,
            &ObjectType::Node,
            Some(props(&[("media.class", "Stream/Output/Audio"), ("client.id", "92")]).as_ref()),
        );
        port(&mut graph, 75, 7, true, "FR");
        graph.describe_node(7, Some(props(&[("client.id", "92")]).as_ref()));
        assert!(
            !heard_from(&graph, 7),
            "this process's PulseAudio playback was heard"
        );
    }

    #[test]
    fn the_offer_is_a_pod() {
        let bytes = format_offer();
        assert!(Pod::from_bytes(&bytes).is_some_and(|pod| pod.is_object()));
    }
}
