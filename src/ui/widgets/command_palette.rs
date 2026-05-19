#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub name: String,
    pub action: String,
}

pub fn default_commands() -> Vec<Command> {
    vec![
        Command {
            name: "Save".to_string(),
            action: "save".to_string(),
        },
        Command {
            name: "Quit".to_string(),
            action: "quit".to_string(),
        },
    ]
}
