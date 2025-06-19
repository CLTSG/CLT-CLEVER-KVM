use enigo::{Enigo, Key, KeyboardControllable, MouseButton, MouseControllable};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum InputEvent {
    #[serde(rename = "mousemove")]
    MouseMove { x: i32, y: i32 },
    
    #[serde(rename = "mousedown")]
    MouseDown { button: String, x: i32, y: i32 },
    
    #[serde(rename = "mouseup")]
    MouseUp { button: String, x: i32, y: i32 },
    
    #[serde(rename = "wheel")]
    MouseWheel { delta_y: i32 },
    
    #[serde(rename = "keydown")]
    KeyDown { key: String, modifiers: Vec<String> },
    
    #[serde(rename = "keyup")]
    KeyUp { key: String, modifiers: Vec<String> },
}

pub struct InputHandler {
    enigo: Enigo,
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            enigo: Enigo::new(),
        }
    }

    pub fn handle_event(&mut self, event: InputEvent) -> Result<(), String> {
        match event {
            InputEvent::MouseMove { x, y } => {
                self.enigo.mouse_move_to(x, y);
            }
            
            InputEvent::MouseDown { button, x, y } => {
                self.enigo.mouse_move_to(x, y);
                let button = self.map_mouse_button(&button)?;
                self.enigo.mouse_down(button);
            }
            
            InputEvent::MouseUp { button, x, y } => {
                self.enigo.mouse_move_to(x, y);
                let button = self.map_mouse_button(&button)?;
                self.enigo.mouse_up(button);
            }
            
            InputEvent::MouseWheel { delta_y } => {
                // Positive delta_y is scroll down, negative is scroll up
                let click_count = (delta_y / 120).abs() as usize;
                for _ in 0..click_count {
                    if delta_y > 0 {
                        self.enigo.mouse_scroll_y(-1);
                    } else {
                        self.enigo.mouse_scroll_y(1);
                    }
                }
            }
            
            InputEvent::KeyDown { key, modifiers } => {
                self.handle_modifiers(&modifiers, true)?;
                let key = self.map_key(&key)?;
                self.enigo.key_down(key);
            }
            
            InputEvent::KeyUp { key, modifiers } => {
                let key = self.map_key(&key)?;
                self.enigo.key_up(key);
                self.handle_modifiers(&modifiers, false)?;
            }
        }
        
        Ok(())
    }

    fn map_mouse_button(&self, button: &str) -> Result<MouseButton, String> {
        match button {
            "left" => Ok(MouseButton::Left),
            "right" => Ok(MouseButton::Right),
            "middle" => Ok(MouseButton::Middle),
            _ => Err(format!("Unsupported mouse button: {}", button)),
        }
    }

    fn map_key(&self, key: &str) -> Result<Key, String> {
        match key {
            "a" => Ok(Key::Layout('a')),
            "b" => Ok(Key::Layout('b')),
            "c" => Ok(Key::Layout('c')),
            "d" => Ok(Key::Layout('d')),
            "e" => Ok(Key::Layout('e')),
            "f" => Ok(Key::Layout('f')),
            "g" => Ok(Key::Layout('g')),
            "h" => Ok(Key::Layout('h')),
            "i" => Ok(Key::Layout('i')),
            "j" => Ok(Key::Layout('j')),
            "k" => Ok(Key::Layout('k')),
            "l" => Ok(Key::Layout('l')),
            "m" => Ok(Key::Layout('m')),
            "n" => Ok(Key::Layout('n')),
            "o" => Ok(Key::Layout('o')),
            "p" => Ok(Key::Layout('p')),
            "q" => Ok(Key::Layout('q')),
            "r" => Ok(Key::Layout('r')),
            "s" => Ok(Key::Layout('s')),
            "t" => Ok(Key::Layout('t')),
            "u" => Ok(Key::Layout('u')),
            "v" => Ok(Key::Layout('v')),
            "w" => Ok(Key::Layout('w')),
            "x" => Ok(Key::Layout('x')),
            "y" => Ok(Key::Layout('y')),
            "z" => Ok(Key::Layout('z')),
            "0" => Ok(Key::Layout('0')),
            "1" => Ok(Key::Layout('1')),
            "2" => Ok(Key::Layout('2')),
            "3" => Ok(Key::Layout('3')),
            "4" => Ok(Key::Layout('4')),
            "5" => Ok(Key::Layout('5')),
            "6" => Ok(Key::Layout('6')),
            "7" => Ok(Key::Layout('7')),
            "8" => Ok(Key::Layout('8')),
            "9" => Ok(Key::Layout('9')),
            "Space" => Ok(Key::Space),
            "Enter" => Ok(Key::Return),
            "Backspace" => Ok(Key::Backspace),
            "Escape" => Ok(Key::Escape),
            "Tab" => Ok(Key::Tab),
            "ArrowUp" => Ok(Key::UpArrow),
            "ArrowDown" => Ok(Key::DownArrow),
            "ArrowLeft" => Ok(Key::LeftArrow),
            "ArrowRight" => Ok(Key::RightArrow),
            // Add more mappings as needed
            _ => {
                if key.len() == 1 {
                    let ch = key.chars().next().unwrap();
                    Ok(Key::Layout(ch))
                } else {
                    Err(format!("Unsupported key: {}", key))
                }
            }
        }
    }

    fn handle_modifiers(&mut self, modifiers: &[String], is_down: bool) -> Result<(), String> {
        for modifier in modifiers {
            let key = match modifier.as_str() {
                "Control" => Key::Control,
                "Alt" => Key::Alt,
                "Shift" => Key::Shift,
                "Meta" | "Command" => Key::Meta,
                _ => return Err(format!("Unsupported modifier: {}", modifier)),
            };
            
            if is_down {
                self.enigo.key_down(key);
            } else {
                self.enigo.key_up(key);
            }
        }
        
        Ok(())
    }
}
