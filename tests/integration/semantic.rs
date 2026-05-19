use jet::highlight::semantic::decode_semantic_tokens;

#[test]
fn semantic_token_delta_decode_produces_spans() {
    let data = vec![
        0, 0, 5, 7, 0, // line 0 col 0 len 5 type 7
        1, 2, 3, 6, 0, // line 1 col 2 len 3 type 6
    ];
    let tokens = decode_semantic_tokens(&data);
    assert_eq!(tokens.tokens().len(), 2);
    assert_eq!(tokens.tokens()[0].line, 0);
    assert_eq!(tokens.tokens()[0].start, 0);
    assert_eq!(tokens.tokens()[1].line, 1);
    assert_eq!(tokens.tokens()[1].start, 2);
}
