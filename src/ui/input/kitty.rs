#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KittyKeyboard {
    pub enabled: bool,
}

impl KittyKeyboard {
    pub fn enable_sequence() -> &'static str {
        "\x1b[>1u"
    }

    pub fn disable_sequence() -> &'static str {
        "\x1b[<u"
    }
}
