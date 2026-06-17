use sdl2::EventPump;
use sdl2::GameControllerSubsystem;
use sdl2::controller::{Axis, Button, GameController};
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

/// Directional state from a single source (keyboard, d-pad or analog stick).
/// Tracked separately per source so that one source releasing a direction does
/// not clear a direction another source is still holding.
#[derive(Debug, Default, Clone)]
struct DirState {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
}

/// Analog sticks rest near zero but rarely exactly zero, so directions only
/// register past this magnitude (axis values span the full i16 range).
const AXIS_DEADZONE: i16 = 8000;

/// SDL-based input for production use. Handles both keyboard and game
/// controllers; the two are merged so either can drive the game and any
/// connected controller works.
pub struct SdlInput {
    event_pump: EventPump,
    controller_subsystem: GameControllerSubsystem,
    /// Open controllers, keyed by instance id. Kept alive so SDL keeps
    /// delivering their events; controllers are opened/closed as they are
    /// plugged in and out.
    controllers: HashMap<u32, GameController>,
    keyboard: DirState,
    dpad: DirState,
    stick: DirState,
    keyboard_jump: bool,
    pad_jump: bool,
    prev_jump: bool,
    quit: bool,
}

impl SdlInput {
    pub fn new(event_pump: EventPump, controller_subsystem: GameControllerSubsystem) -> Self {
        Self {
            event_pump,
            controller_subsystem,
            controllers: HashMap::new(),
            keyboard: DirState::default(),
            dpad: DirState::default(),
            stick: DirState::default(),
            keyboard_jump: false,
            pad_jump: false,
            prev_jump: false,
            quit: false,
        }
    }
}

impl InputSource for SdlInput {
    fn poll(&mut self) -> InputState {
        for event in self.event_pump.poll_iter() {
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => self.quit = true,
                Event::KeyDown {
                    keycode: Some(key), ..
                } => match key {
                    Keycode::Up | Keycode::W => self.keyboard.up = true,
                    Keycode::Down | Keycode::S => self.keyboard.down = true,
                    Keycode::Left | Keycode::A => self.keyboard.left = true,
                    Keycode::Right | Keycode::D => self.keyboard.right = true,
                    Keycode::Space => self.keyboard_jump = true,
                    _ => {}
                },
                Event::KeyUp {
                    keycode: Some(key), ..
                } => match key {
                    Keycode::Up | Keycode::W => self.keyboard.up = false,
                    Keycode::Down | Keycode::S => self.keyboard.down = false,
                    Keycode::Left | Keycode::A => self.keyboard.left = false,
                    Keycode::Right | Keycode::D => self.keyboard.right = false,
                    Keycode::Space => self.keyboard_jump = false,
                    _ => {}
                },
                Event::ControllerDeviceAdded { which, .. } => {
                    if let Ok(controller) = self.controller_subsystem.open(which) {
                        self.controllers.insert(controller.instance_id(), controller);
                    }
                }
                Event::ControllerDeviceRemoved { which, .. } => {
                    self.controllers.remove(&which);
                }
                Event::ControllerButtonDown { button, .. } => match button {
                    Button::DPadUp => self.dpad.up = true,
                    Button::DPadDown => self.dpad.down = true,
                    Button::DPadLeft => self.dpad.left = true,
                    Button::DPadRight => self.dpad.right = true,
                    // A/B (or X/Y) all jump, so the binding feels natural on any layout.
                    Button::A | Button::B | Button::X | Button::Y => self.pad_jump = true,
                    Button::Start => self.quit = true,
                    _ => {}
                },
                Event::ControllerButtonUp { button, .. } => match button {
                    Button::DPadUp => self.dpad.up = false,
                    Button::DPadDown => self.dpad.down = false,
                    Button::DPadLeft => self.dpad.left = false,
                    Button::DPadRight => self.dpad.right = false,
                    Button::A | Button::B | Button::X | Button::Y => self.pad_jump = false,
                    _ => {}
                },
                Event::ControllerAxisMotion { axis, value, .. } => match axis {
                    Axis::LeftX => {
                        self.stick.left = value < -AXIS_DEADZONE;
                        self.stick.right = value > AXIS_DEADZONE;
                    }
                    Axis::LeftY => {
                        self.stick.up = value < -AXIS_DEADZONE;
                        self.stick.down = value > AXIS_DEADZONE;
                    }
                    _ => {}
                },
                _ => {}
            }
        }

        let jump = self.keyboard_jump || self.pad_jump;
        let state = InputState {
            up: self.keyboard.up || self.dpad.up || self.stick.up,
            down: self.keyboard.down || self.dpad.down || self.stick.down,
            left: self.keyboard.left || self.dpad.left || self.stick.left,
            right: self.keyboard.right || self.dpad.right || self.stick.right,
            jump,
            // Edge-detect a jump press across all sources.
            jump_pressed: jump && !self.prev_jump,
            quit: self.quit,
        };
        self.prev_jump = jump;
        state
    }

    fn should_quit(&self) -> bool {
        self.quit
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
