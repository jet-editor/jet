use jet::{
    buffer::rope::EditorBuffer,
    editor::textobjects,
    highlight::treesitter::{Language, TreeSitterHighlighter},
};

#[test]
fn rust_function_textobject_selects_surrounding_function() {
    let source = "fn outer() {\n    fn inner() {\n        let x = 1;\n    }\n}\n";
    let buffer = EditorBuffer::from_text(source);
    let mut highlighter = TreeSitterHighlighter::new(Language::Rust);
    assert!(highlighter.parse_buffer(&buffer));
    let tree = highlighter.tree().expect("parse tree");
    let inner_call = source.find("let x").unwrap();
    let range = textobjects::function_around(&buffer, Language::Rust, tree, inner_call)
        .expect("function textobject");
    let selected = buffer.slice_chars(range.start, range.end);
    assert!(
        selected.contains("fn inner"),
        "expected inner function, got: {selected}"
    );
}
