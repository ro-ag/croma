pub fn format_abc(source: &str) -> String {
    let mut output = String::new();
    for line in source.lines() {
        output.push_str(line.trim_end());
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_trailing_whitespace() {
        assert_eq!(format_abc("X:1  \nK:C\t\nC   \n"), "X:1\nK:C\nC\n");
    }
}
