use anyhow::{Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
};

pub struct TerminalPanel {
    writer: Box<dyn Write + Send>,
    output: Arc<Mutex<Vec<String>>>,
    rows: u16,
}

impl TerminalPanel {
    pub fn spawn(rows: u16, cols: u16) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let shell = std::env::var("COMSPEC")
            .or_else(|_| std::env::var("SHELL"))
            .unwrap_or_else(|_| {
                if cfg!(windows) {
                    "cmd.exe".to_string()
                } else {
                    "/bin/sh".to_string()
                }
            });
        let cmd = CommandBuilder::new(shell);
        let _child = pair
            .slave
            .spawn_command(cmd)
            .context("spawn shell in pty")?;
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let output = Arc::new(Mutex::new(vec!["(terminal ready)".to_string()]));
        let output_reader = Arc::clone(&output);
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(count) => {
                        let chunk = String::from_utf8_lossy(&buf[..count]);
                        let mut lines = output_reader.lock().expect("terminal output lock");
                        for segment in chunk.split(['\n', '\r']) {
                            if segment.is_empty() {
                                continue;
                            }
                            if let Some(last) = lines.last_mut() {
                                if !chunk.contains('\n') && !chunk.contains('\r') {
                                    last.push_str(segment);
                                    continue;
                                }
                            }
                            lines.push(segment.to_string());
                        }
                        if lines.len() > 500 {
                            let drain = lines.len() - 500;
                            lines.drain(0..drain);
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            writer,
            output,
            rows,
        })
    }

    pub fn write_input(&mut self, text: &str) -> Result<()> {
        self.writer
            .write_all(text.as_bytes())
            .context("write to pty")?;
        self.writer.flush().context("flush pty")?;
        Ok(())
    }

    pub fn visible_lines(&self, max_rows: usize) -> Vec<String> {
        let lines = self.output.lock().expect("terminal output lock");
        if lines.len() <= max_rows {
            return lines.clone();
        }
        lines[lines.len() - max_rows..].to_vec()
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }
}
