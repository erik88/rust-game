use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::EventPump;

#[derive(Debug, Default)]
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

pub struct InputHandler {
    state: InputState,
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            state: InputState::new(),
        }
    }

    pub fn update(&mut self, event_pump: &mut EventPump) {
        // Reset jump_pressed flag at the start of each frame
        self.state.jump_pressed = false;

        for event in event_pump.poll_iter() {
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
    }

    pub fn state(&self) -> &InputState {
        &self.state
    }

    pub fn should_quit(&self) -> bool {
        self.state.quit
    }
}