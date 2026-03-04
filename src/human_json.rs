//! Human Json: strip comments and remove trailing comma's

use std::str::Chars;

use log::warn;

pub fn preprocess_human_json(json: String) -> String {
    let preprocessor = JsonPreprocessor::new(&json);

    match preprocessor.preprocess() {
        Ok(value) => value,
        Err(err) => {
            warn!(
                "failed to preprocess json (e.g. remove comments, trailing commas), json might be invalid: {err}"
            );
            json
        }
    }
}

struct JsonPreprocessor<'a> {
    iter: Chars<'a>,
    peek_len: usize,
    peek: [char; 2],
    new_string: String,
}

enum CommentState {
    None,
    /// `# Comment` or `// Comment`
    SingleLine,
    /// `/* Comment */`
    MultiLine,
}

impl<'a> JsonPreprocessor<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            iter: text.chars(),
            peek_len: 0,
            peek: [' ', ' '],
            new_string: String::new(),
        }
    }

    pub fn preprocess(mut self) -> Result<String, anyhow::Error> {
        while self.validate_next()? {}

        Ok(self.new_string)
    }

    /// Bool says if it should continue
    fn validate_next(&mut self) -> Result<bool, anyhow::Error> {
        self.validate_empty()?;

        let Some([next_char]) = self.peek()? else {
            return Ok(false);
        };

        match next_char {
            '"' => {
                self.next::<1>(None)?;

                let mut escaped = false;
                loop {
                    let Some([next_char]) = self.peek()? else {
                        return Ok(false);
                    };

                    if !escaped {
                        if next_char == '\\' {
                            escaped = true;
                        }
                        if next_char == '"' {
                            self.next::<1>(None)?;
                            break;
                        }
                    } else {
                        escaped = false;
                    }

                    self.next::<1>(None)?;
                }
            }
            ',' => {
                let maybe_comma_index = self.new_string.len();
                self.next::<1>(Some([' ']))?; // <-- Remove comma

                self.validate_empty()?;

                let Some([next_char]) = self.peek()? else {
                    return Ok(false);
                };

                match next_char {
                    '}' | ']' => {
                        // Fallthrough: Comma is already removed
                    }
                    _ => {
                        self.new_string
                            .replace_range(maybe_comma_index..(maybe_comma_index + 1), ",");
                    }
                }
            }
            // By default just go on
            _ => {
                self.next::<1>(None)?;
            }
        };
        Ok(true)
    }

    fn validate_empty(&mut self) -> Result<(), anyhow::Error> {
        let mut state = CommentState::None;

        loop {
            match state {
                CommentState::None => {
                    let Some(next_chars) = self.peek::<2>()? else {
                        return Ok(());
                    };

                    match next_chars {
                        ['#', _] => {
                            state = CommentState::SingleLine;

                            self.next::<1>(Some([' ']))?;
                        }
                        ['/', '/'] => {
                            state = CommentState::SingleLine;

                            self.next::<2>(Some([' ', ' ']))?;
                        }
                        ['/', '*'] => {
                            state = CommentState::MultiLine;

                            self.next::<2>(Some([' ', ' ']))?;
                        }
                        [' ', _] | ['\n', _] | ['\r', _] => {
                            self.next::<1>(None)?;
                        }
                        _ => return Ok(()),
                    }
                }
                CommentState::SingleLine => {
                    let Some([next_char]) = self.peek()? else {
                        return Ok(());
                    };

                    if matches!(next_char, '\n' | '\r') {
                        state = CommentState::None;

                        self.next::<1>(None)?;
                        continue;
                    }

                    self.next::<1>(Some([' ']))?;
                }
                CommentState::MultiLine => {
                    let Some(next_chars) = self.peek::<2>()? else {
                        return Ok(());
                    };

                    if next_chars == ['*', '/'] {
                        state = CommentState::None;

                        self.next(Some([' ', ' ']))?;
                        continue;
                    }

                    if matches!(next_chars[0], '\n' | '\r') {
                        self.next::<1>(None)?;
                    } else {
                        self.next::<1>(Some([' ']))?;
                    }
                }
            }
        }
    }

    fn peek<const N: usize>(&mut self) -> Result<Option<[char; N]>, anyhow::Error> {
        let mut chars = [' '; N];

        while self.peek_len < N {
            let Some(peek) = self.iter.next() else {
                return Ok(None);
            };

            self.peek[self.peek_len] = peek;
            self.peek_len += 1;
        }

        chars.copy_from_slice(&self.peek[0..N]);

        Ok(Some(chars))
    }
    fn next<const N: usize>(
        &mut self,
        insert_instead: Option<[char; N]>,
    ) -> Result<Option<[char; N]>, anyhow::Error> {
        let result = self.peek()?;

        if let Some(actual) = result {
            match insert_instead {
                Some(insert_instead) => {
                    self.new_string.extend(insert_instead);
                }
                None => {
                    self.new_string.extend(actual);
                }
            }
        }

        self.peek_len -= N;
        self.peek.copy_within(N.., 0);

        Ok(result)
    }
}

#[cfg(test)]
mod test {
    use crate::human_json::preprocess_human_json;

    #[test]
    fn test_empty_json() {
        let human = r#"{}"#.to_string();
        let expected = r#"{}"#;

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_whitespace_handling() {
        let human = r#"   {   "key"  :  "value"   }   "#.to_string();
        let expected = r#"   {   "key"  :  "value"   }   "#;

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_multiple_comments_in_one_line() {
        let human = r#"key: value # comment1 here // comment2 here"#.to_string();
        let expected = r#"key: value                                 "#;

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_key_value() {
        let human = r#"{"test":"test"}"#.to_string();
        let expected = r#"{"test":"test"}"#;

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_escape_quotes() {
        let human = r#"test\""test"#.to_string();
        let expected = r#"test\""test"#;

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_new_lines() {
        let human = r#"{
"test"
:
"test"
}"#
        .to_string();
        let expected = r#"{
"test"
:
"test"
}"#;

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_text_escape() {
        let human = r#""\t\e\s\t""#.to_string();
        let expected = r#""\t\e\s\t""#;

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_comment_hashtag() {
        let human = "hello#comment\nworld".to_string();
        let expected = "hello        \nworld";

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_comment_slash() {
        let human = "hello//comment\nworld".to_string();
        let expected = "hello         \nworld";

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_comment_multiline() {
        let human = "hello/*com\n\rment*/world".to_string();
        let expected = "hello     \n\r      world";

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_multiline_comments() {
        let human = r#"key: value // comment line 1
                  // comment line 2
                  // comment line 3"#
            .to_string();
        let expected = r#"key: value                  
                                   
                                   "#;

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_json_array_with_comments() {
        let human = r#"[1, 2, 3, // comment
4, 5, 6]"#
            .to_string();
        let expected = r#"[1, 2, 3,           
4, 5, 6]"#;

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_json_array_remove_trailing_comma() {
        let human = "[1, 2, 3, 4, 5, 6,   \n ]".to_string();
        let expected = "[1, 2, 3, 4, 5, 6    \n ]".to_string();

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_json_array_packed() {
        let human = "[1,2,3,4,5,6,]".to_string();
        let expected = "[1,2,3,4,5,6 ]".to_string();

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_json_array_remove_trailing_comma2() {
        let human = r#"{
1 ,
2,
}"#
        .to_string();
        let expected = r#"{
1 ,
2 
}"#;

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_json_object_packed() {
        let human = r#"{"key":"value","key":"value",}"#.to_string();
        let expected = r#"{"key":"value","key":"value" }"#;

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_json_object_remove_trailing_comma() {
        let human = r#"{
"key": "value" ,
"key": "value",
}"#
        .to_string();
        let expected = r#"{
"key": "value" ,
"key": "value" 
}"#;

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_single_line_comment_in_object() {
        let human = r#"{
        "key1": "value1", // this is a comment
        "key2": "value2"
    }"#
        .to_string();
        let expected = r#"{
        "key1": "value1",                     
        "key2": "value2"
    }"#;

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_multiple_trailing_commas_in_object() {
        let human = r#"{
        "key1": "value1",
        "key2": "value2",
        "key3": "value3",
    }"#
        .to_string();
        let expected = r#"{
        "key1": "value1",
        "key2": "value2",
        "key3": "value3" 
    }"#;

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_comment_inside_string_value() {
        let human = r#"{
        "message": "this is a comment: // not a real comment"
    }"#
        .to_string();
        let expected = r#"{
        "message": "this is a comment: // not a real comment"
    }"#;

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_trailing_comma_with_comment() {
        let human = r#"{
        "key1": "value1", // this is a comment
        "key2": "value2",
    }"#
        .to_string();
        let expected = r#"{
        "key1": "value1",                     
        "key2": "value2" 
    }"#;

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }

    #[test]
    fn test_complex_json_with_comments_and_commas() {
        let human = r#"
    {
        "key1": "value1", // comment after key1
        "key2": "value2", /* multi-line 
        comment 
        after key2 */
        "key3": "value3", 
    }"#
        .to_string();
        let expected = r#"
    {
        "key1": "value1",                      
        "key2": "value2",               
                
                     
        "key3": "value3"  
    }"#;

        assert_eq!(preprocess_human_json(human).as_str(), expected);
    }
}
