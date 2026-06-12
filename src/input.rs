use sdl2::EventPump;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct InputState {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub jump_pressed: bool,
    pub quit: bool,
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Trait for abstracting input sources
pub trait InputSource {
    /// Update and return the current input state
    fn poll(&mut self) -> InputState;
    /// Check if the application should quit
    fn should_quit(&self) -> bool;
}

/// SDL-based input for production use
pub struct SdlInput {
    event_pump: EventPump,
    state: InputState,
}

impl SdlInput {
    pub fn new(event_pump: EventPump) -> Self {
        Self {
            event_pump,
            state: InputState::new(),
        }
    }
}

impl InputSource for SdlInput {
    fn poll(&mut self) -> InputState {
        // Reset jump_pressed flag at the start of each frame
        self.state.jump_pressed = false;

        for event in self.event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => self.state.quit = true,
                Event::KeyDown {
                    keycode: Some(key), ..
                } => match key {
                    Keycode::Up | Keycode::W => self.state.up = true,
                    Keycode::Down | Keycode::S => self.state.down = true,
                    Keycode::Left | Keycode::A => self.state.left = true,
                    Keycode::Right | Keycode::D => self.state.right = true,
                    Keycode::Space => {
                        if !self.state.jump {
                            self.state.jump_pressed = true;
                        }
                        self.state.jump = true;
                    }
                    _ => {}
                },
                Event::KeyUp {
                    keycode: Some(key), ..
                } => match key {
                    Keycode::Up | Keycode::W => self.state.up = false,
                    Keycode::Down | Keycode::S => self.state.down = false,
                    Keycode::Left | Keycode::A => self.state.left = false,
                    Keycode::Right | Keycode::D => self.state.right = false,
                    Keycode::Space => self.state.jump = false,
                    _ => {}
                },
                _ => {}
            }
        }

        self.state.clone()
    }

    fn should_quit(&self) -> bool {
        self.state.quit
    }
}

/// Queued input for testing - provides pre-programmed input sequences
pub struct QueuedInput {
    inputs: HashMap<usize, InputState>,
    current_frame: usize,
    default_state: InputState,
}

impl QueuedInput {
    pub fn new() -> Self {
        Self {
            inputs: HashMap::new(),
            current_frame: 0,
            default_state: InputState::new(),
        }
    }

    /// Queue an input state for a specific frame number
    pub fn queue_input(&mut self, frame: usize, state: InputState) {
        self.inputs.insert(frame, state);
    }
}

impl InputSource for QueuedInput {
    fn poll(&mut self) -> InputState {
        let state = self
            .inputs
            .get(&self.current_frame)
            .cloned()
            .unwrap_or_else(|| self.default_state.clone());

        self.current_frame += 1;
        state
    }

    fn should_quit(&self) -> bool {
        false // Tests never quit via input
    }
}
