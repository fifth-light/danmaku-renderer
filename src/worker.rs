use std::{
    any::Any,
    error::Error,
    fmt::Display,
    sync::{
        mpsc::{channel, Receiver, SendError, Sender},
        Arc, Mutex,
    },
    thread::{spawn, JoinHandle},
    time::{Duration, Instant},
};

use cosmic_text::{AttrsList, FontSystem, ShapeBuffer};
use log::{debug, log_enabled, warn, Level::Debug};

use crate::{
    layout::LayoutMode,
    manager::{DanmakuTimeChunk, DanmakuTimeChunkProvider},
    sources::DanmakuSource,
};

pub trait RenderCache: Sync + Send {
    fn new_param(&mut self, new_param: DanmakuParam);
    fn prepare(&mut self, font_system: &mut FontSystem, chunk: &DanmakuTimeChunk);
    fn flush(&mut self) {}
}

pub trait ChunkBuffer<Cache: RenderCache>: Sync + Send {
    fn new(chunk: &Arc<DanmakuTimeChunk>, cache: &mut Cache) -> Arc<Self>;
    fn index(&self) -> u32;
    fn base_state_index(&self) -> u32;
}

impl<Cache: RenderCache> ChunkBuffer<Cache> for DanmakuTimeChunk {
    fn new(chunk: &Arc<DanmakuTimeChunk>, _cache: &mut Cache) -> Arc<Self> {
        chunk.clone()
    }

    fn index(&self) -> u32 {
        self.index
    }

    fn base_state_index(&self) -> u32 {
        self.base_state_index
    }
}

#[derive(Debug)]
enum WorkerRequest {
    Chunk(Option<u32>, u32),
    Stop,
}

pub struct WorkerBuffer<Cache, Chunk>
where
    Cache: RenderCache,
    Chunk: ChunkBuffer<Cache>,
{
    pub cache: Cache,
    pub previous: Option<Arc<Chunk>>,
    pub current: Option<Arc<Chunk>>,
    pub next: Option<Arc<Chunk>>,
}

impl<Cache, Chunk> Default for WorkerBuffer<Cache, Chunk>
where
    Cache: RenderCache + Default,
    Chunk: ChunkBuffer<Cache>,
{
    fn default() -> Self {
        Self {
            cache: Default::default(),
            previous: None,
            current: None,
            next: None,
        }
    }
}

impl<Cache, Chunk> WorkerBuffer<Cache, Chunk>
where
    Cache: RenderCache,
    Chunk: ChunkBuffer<Cache>,
{
    pub fn new(cache: Cache) -> Self {
        WorkerBuffer {
            cache,
            previous: None,
            current: None,
            next: None,
        }
    }

    pub fn should_request_worker(&self, index: u32) -> bool {
        if let Some((_, current)) = self.previous.as_ref().zip(self.current.as_ref()) {
            return current.index() != index;
        }
        if let Some((current, _)) = self.current.as_ref().zip(self.next.as_ref()) {
            if current.index() == index {
                return false;
            }
        }
        true
    }

    pub fn acquire_index(&self, index: u32) -> Option<(&Chunk, &Chunk)> {
        if let Some((previous, current)) = self.previous.as_ref().zip(self.current.as_ref()) {
            if current.index() == index {
                return Some((previous, current));
            }
        }
        if let Some((current, next)) = self.current.as_ref().zip(self.next.as_ref()) {
            if current.index() == index {
                return Some((current, next));
            }
            if next.index() == index {
                return Some((current, next));
            }
        }
        None
    }
}

pub struct WorkerState<Cache, Chunk>
where
    Cache: RenderCache,
    Chunk: ChunkBuffer<Cache>,
{
    pub buffer: Arc<Mutex<WorkerBuffer<Cache, Chunk>>>,
    pub font_system: FontSystem,
    pub shape_buffer: ShapeBuffer,
    pub source: Box<dyn DanmakuSource + Send>,
}

#[derive(Clone, Debug)]
pub struct DanmakuParam {
    pub screen_size: (u32, u32),
    pub lifetime: Duration,
    pub font_size: f32,
    pub line_height: u32,
    pub font_attrs: AttrsList,
    pub layout_mode: LayoutMode,
    pub shadow_size: u32,
    pub shadow_weight: f32,
}

type WorkerCallback<Cache, Chunk> = (Receiver<WorkerRequest>, WorkerState<Cache, Chunk>);

fn worker_thread<Cache, Chunk>(
    rx: Receiver<WorkerRequest>,
    param: DanmakuParam,
    mut state: WorkerState<Cache, Chunk>,
) -> WorkerCallback<Cache, Chunk>
where
    Cache: RenderCache,
    Chunk: ChunkBuffer<Cache>,
{
    let mut provider = DanmakuTimeChunkProvider::new(
        param.lifetime,
        param.font_size,
        param.font_attrs,
        param.screen_size,
        param.line_height,
        param.layout_mode,
        state.source,
    );
    loop {
        let request = rx.recv();
        if let Ok(request) = &request {
            debug!("Worker request: {:?}", request);
        }
        match request {
            Ok(WorkerRequest::Chunk(start, now)) => {
                let start_time = if log_enabled!(Debug) {
                    Some(Instant::now())
                } else {
                    None
                };
                let mut start = start;
                let previous = if now > 0 {
                    let chunk = provider.get_chunk(
                        &mut state.font_system,
                        &mut state.shape_buffer,
                        start,
                        now - 1,
                    );
                    let chunk = match chunk {
                        Ok(chunk) => chunk,
                        Err(err) => {
                            warn!("Fetch chunk failed: {:?}", err);
                            continue;
                        }
                    };
                    start = Some(chunk.base_state_index);
                    debug!("Generated chunk #{}", now - 1);
                    Some(chunk)
                } else {
                    None
                };

                let current =
                    provider.get_chunk(&mut state.font_system, &mut state.shape_buffer, start, now);
                let current = match current {
                    Ok(current) => {
                        debug!("Generated chunk #{}", now);
                        current
                    }
                    Err(err) => {
                        warn!("Fetch chunk failed: {:?}", err);
                        continue;
                    }
                };

                let next = provider.get_chunk(
                    &mut state.font_system,
                    &mut state.shape_buffer,
                    start,
                    now + 1,
                );
                let next = match next {
                    Ok(next) => {
                        debug!("Generated chunk #{}", now + 1);
                        next
                    }
                    Err(err) => {
                        warn!("Fetch chunk failed: {:?}", err);
                        continue;
                    }
                };

                let mut buffer = state.buffer.lock().unwrap();
                if let Some(previous) = &previous {
                    buffer.cache.prepare(&mut state.font_system, previous)
                }
                buffer.cache.prepare(&mut state.font_system, &current);
                buffer.cache.prepare(&mut state.font_system, &next);
                buffer.cache.flush();
                buffer.previous = previous.map(|previous| Chunk::new(&previous, &mut buffer.cache));
                buffer.current = Some(Chunk::new(&current, &mut buffer.cache));
                buffer.next = Some(Chunk::new(&next, &mut buffer.cache));
                drop(buffer);
                if let Some(start_time) = start_time {
                    let generate_time = start_time.elapsed();
                    debug!("Generated chunk #{}, time: {:?}", now, generate_time);
                }
            }
            Ok(WorkerRequest::Stop) => break,
            Err(_) => {
                warn!("Receive message from main thread failed, is main thread dead?");
                break;
            }
        }
    }
    (
        rx,
        WorkerState {
            source: provider.source(),
            ..state
        },
    )
}

#[derive(Debug)]
pub enum WorkerError {
    JoinError,
    SendError,
}

impl Display for WorkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkerError::JoinError => write!(f, "Worker thread panicked"),
            WorkerError::SendError => write!(f, "Failed to send message to worker"),
        }
    }
}

impl Error for WorkerError {}

impl From<Box<dyn Any + Send + 'static>> for WorkerError {
    fn from(_: Box<dyn Any + Send + 'static>) -> Self {
        WorkerError::JoinError
    }
}

impl From<SendError<WorkerRequest>> for WorkerError {
    fn from(_: SendError<WorkerRequest>) -> Self {
        WorkerError::SendError
    }
}

pub struct WorkerManager<Cache, Chunk>
where
    Cache: RenderCache,
    Chunk: ChunkBuffer<Cache>,
{
    sender: Sender<WorkerRequest>,
    thread_handle: Mutex<Option<JoinHandle<WorkerCallback<Cache, Chunk>>>>,
    last_request: Option<(Option<u32>, u32)>,
}

impl<Cache, Chunk> WorkerManager<Cache, Chunk>
where
    Cache: RenderCache + 'static,
    Chunk: ChunkBuffer<Cache> + 'static,
{
    pub fn new(param: DanmakuParam, state: WorkerState<Cache, Chunk>) -> Self {
        let (sender, receiver) = channel();
        let thread_handle = spawn(move || worker_thread(receiver, param, state));
        WorkerManager {
            sender,
            thread_handle: Mutex::new(Some(thread_handle)),
            last_request: None,
        }
    }

    pub fn request(
        &mut self,
        state_begin_index: Option<u32>,
        index: u32,
    ) -> Result<(), impl Error> {
        if Some((state_begin_index, index)) == self.last_request {
            return Ok(());
        }
        let request = WorkerRequest::Chunk(state_begin_index, index);
        self.sender.send(request)?;
        self.last_request = Some((state_begin_index, index));
        Ok::<(), SendError<_>>(())
    }

    pub fn change_param(&mut self, new_param: DanmakuParam) -> Result<(), WorkerError> {
        self.sender.send(WorkerRequest::Stop)?;
        let mut thread_handle = self.thread_handle.lock().unwrap();
        let handle = thread_handle.take();
        let handle = match handle {
            Some(handle) => handle,
            None => {
                warn!("Thread handle is none");
                return Ok(());
            }
        };
        let (receiver, state) = handle.join()?;

        let mut state_lock = state.buffer.lock().unwrap();
        state_lock.cache.new_param(new_param.clone());
        drop(state_lock);

        let new_thread_handle = spawn(move || worker_thread(receiver, new_param, state));
        *thread_handle = Some(new_thread_handle);
        if let Some(last_request) = self.last_request {
            self.sender
                .send(WorkerRequest::Chunk(last_request.0, last_request.1))?;
        }
        Ok(())
    }

    pub fn into_state(self) -> Result<WorkerState<Cache, Chunk>, WorkerError> {
        self.sender.send(WorkerRequest::Stop)?;
        let mut thread_handle = self.thread_handle.lock().unwrap();
        let (_, state) = thread_handle.take().unwrap().join()?;
        Ok(state)
    }
}

impl<Cache, Chunk> Drop for WorkerManager<Cache, Chunk>
where
    Cache: RenderCache,
    Chunk: ChunkBuffer<Cache>,
{
    fn drop(&mut self) {
        let thread_handle = self.thread_handle.lock().unwrap();
        if thread_handle.is_none() {
            return;
        }
        let _result = self.sender.send(WorkerRequest::Stop);
    }
}
