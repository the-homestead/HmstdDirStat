//! Parse errors: every message, with the byte position it must point at.

mod common;

use common::{check_error, parse_error};
use hmstd_search::compile;

#[test]
fn unknown_field_is_a_parse_error_at_its_term() {
    check_error("foo:bar", 0, "unknown field 'foo:'");
    check_error("ext:iso bogus:1", 8, "unknown field 'bogus:'");
    check_error("ext:iso (bogus:1)", 9, "unknown field 'bogus:'");
}

#[test]
fn regex_term_is_not_supported_yet() {
    // The regex: term is deferred until the dependency choice is confirmed.
    check_error("re:foo.*", 0, "unknown field 're:'");
}

#[test]
fn missing_value_after_a_field() {
    check_error("size:", 0, "missing value after 'size:'");
    check_error("ext:", 0, "missing value after 'ext:'");
    check_error("name:", 0, "missing value after 'name:'");
}

#[test]
fn ext_list_with_no_extensions() {
    check_error("ext:,", 0, "ext: needs at least one extension");
}

#[test]
fn unknown_type_and_attr_values() {
    check_error("type:weird", 0, "unknown type 'weird'");
    check_error("attr:bogus", 0, "unknown attr 'bogus'");
}

#[test]
fn bad_size_values() {
    check_error("size:abc", 0, "expected a size");
    check_error("size:10zb", 0, "unknown size unit 'zb'");
    check_error("size:2gb..1gb", 0, "starts above its end");
}

#[test]
fn bad_dates_and_ages() {
    check_error("modified:2024-02-30", 0, "not a real calendar date");
    check_error("modified:2024-13-01", 0, "not a real calendar date");
    check_error("modified:2024-1", 0, "expected a date");
    check_error("modified:0000-01-01", 0, "not a real calendar date");
    check_error("modified:99999-01-01", 0, "not a real calendar date");
    check_error("modified:2024-06-30..2024-01-01", 0, "starts after its end");
    // Ages that overflow are rejected rather than wrapping or panicking.
    check_error("modified:99999999999999999999y", 0, "expected a date");
    check_error("modified:9000000000000000y", 0, "expected a date");
}

#[test]
fn unclosed_parenthesis_points_at_the_open_paren() {
    check_error("(ext:iso", 0, "missing ')' for this '('");
    check_error("ext:iso (ext:zip", 8, "missing ')' for this '('");
}

#[test]
fn unexpected_close_paren() {
    check_error("ext:iso)", 7, "unexpected ')'");
}

#[test]
fn empty_group_and_dangling_operators() {
    check_error("()", 1, "expected a search term here");
    check_error("OR ext:iso", 0, "expected a search term here");
    check_error("ext:iso OR", 10, "expected a search term");
    check_error("ext:iso NOT", 11, "expected a search term");
}

#[test]
fn unclosed_and_empty_quotes() {
    check_error("\"unterminated", 0, "missing closing quote");
    check_error("path:\"My Games", 5, "missing closing quote");
    check_error("\"\"", 0, "empty quoted term");
}

#[test]
fn error_positions_are_byte_offsets_after_multibyte_text() {
    // 'é' is two bytes, so "café " is 6 bytes and the bad field starts at byte 6.
    check_error("café bogus:1", 6, "unknown field 'bogus:'");
}

#[test]
fn errors_display_their_position() {
    let error = parse_error("size:");
    assert_eq!(
        error.to_string(),
        "missing value after 'size:' (at position 0)"
    );
    assert!(compile("size:10mb").is_ok());
}
