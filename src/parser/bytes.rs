/*
 * hurl (https://hurl.dev)
 * Copyright (C) 2020 Orange
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *          http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 */
use crate::core::ast::*;

use super::base64;
use super::combinators::*;
use super::json::parse as parse_json;
use super::primitives::*;
use super::reader::Reader;
use super::xml;
use super::ParseResult;

pub fn bytes(reader: &mut Reader) -> ParseResult<'static, Bytes> {
    //let start = p.state.clone();
    choice(
        vec![raw_string, json_bytes, xml_bytes, base64_bytes, file_bytes],
        reader,
    )
}

fn xml_bytes(reader: &mut Reader) -> ParseResult<'static, Bytes> {
    match xml::parse(reader) {
        Err(e) => Err(e),
        Ok(value) => Ok(Bytes::Xml { value }),
    }
}

fn json_bytes(reader: &mut Reader) -> ParseResult<'static, Bytes> {
    match parse_json(reader) {
        Err(e) => Err(e),
        Ok(value) => Ok(Bytes::Json { value }),
    }
}

fn file_bytes(reader: &mut Reader) -> ParseResult<'static, Bytes> {
    let _start = reader.state.clone();
    try_literal("file", reader)?;
    literal(",", reader)?;
    let space0 = zero_or_more_spaces(reader)?;
    let f = filename(reader)?;
    let space1 = zero_or_more_spaces(reader)?;
    literal(";", reader)?;
    Ok(Bytes::File {
        space0,
        filename: f,
        space1,
    })
}

fn base64_bytes(reader: &mut Reader) -> ParseResult<'static, Bytes> {
    // base64 => can have whitespace
    // support pqrser position
    let _start = reader.state.clone();
    try_literal("base64", reader)?;
    literal(",", reader)?;
    let space0 = zero_or_more_spaces(reader)?;
    let save_state = reader.state.clone();
    let value = base64::parse(reader);
    let count = reader.state.cursor - save_state.cursor;
    reader.state = save_state;
    let encoded = reader.read_n(count);
    let space1 = zero_or_more_spaces(reader)?;
    literal(";", reader)?;
    Ok(Bytes::Base64 {
        space0,
        value,
        encoded,
        space1,
    })
}

#[cfg(test)]
mod tests {
    use crate::core::common::{Pos, SourceInfo};
    use crate::core::json;

    use super::super::error::*;
    use super::*;

    #[test]
    fn test_bytes_json() {
        let mut reader = Reader::init("[1,2,3] ");
        assert_eq!(
            bytes(&mut reader).unwrap(),
            Bytes::Json {
                value: json::Value::List {
                    space0: "".to_string(),
                    elements: vec![
                        json::ListElement {
                            space0: "".to_string(),
                            value: json::Value::Number("1".to_string()),
                            space1: "".to_string()
                        },
                        json::ListElement {
                            space0: "".to_string(),
                            value: json::Value::Number("2".to_string()),
                            space1: "".to_string()
                        },
                        json::ListElement {
                            space0: "".to_string(),
                            value: json::Value::Number("3".to_string()),
                            space1: "".to_string()
                        },
                    ],
                }
            }
        );
        assert_eq!(reader.state.cursor, 7);

        let mut reader = Reader::init("{ } ");
        assert_eq!(
            bytes(&mut reader).unwrap(),
            Bytes::Json {
                value: json::Value::Object {
                    space0: " ".to_string(),
                    elements: vec![],
                }
            }
        );
        assert_eq!(reader.state.cursor, 3);

        let mut reader = Reader::init("true");
        assert_eq!(
            bytes(&mut reader).unwrap(),
            Bytes::Json {
                value: json::Value::Boolean(true)
            }
        );
        assert_eq!(reader.state.cursor, 4);

        let mut reader = Reader::init("\"\" x");
        assert_eq!(
            bytes(&mut reader).unwrap(),
            Bytes::Json {
                value: json::Value::String(Template {
                    quotes: true,
                    elements: vec![],
                    source_info: SourceInfo::init(1, 2, 1, 2),
                })
            }
        );
        assert_eq!(reader.state.cursor, 2);
    }

    #[test]
    fn test_bytes_xml() {
        let mut reader = Reader::init("<a/>");
        assert_eq!(
            bytes(&mut reader).unwrap(),
            Bytes::Xml {
                value: String::from("<a/>")
            }
        );
    }

    #[test]
    fn test_bytes_file() {
        let mut reader = Reader::init("file,data.xml;");
        assert_eq!(
            bytes(&mut reader).unwrap(),
            Bytes::File {
                space0: Whitespace {
                    value: String::from(""),
                    source_info: SourceInfo::init(1, 6, 1, 6),
                },
                filename: Filename {
                    value: String::from("data.xml"),
                    source_info: SourceInfo::init(1, 6, 1, 14),
                },
                space1: Whitespace {
                    value: String::from(""),
                    source_info: SourceInfo::init(1, 14, 1, 14),
                },
            }
        );
    }

    #[test]
    fn test_bytes_json_error() {
        let mut reader = Reader::init("{ x ");
        let error = bytes(&mut reader).err().unwrap();
        assert_eq!(error.pos, Pos { line: 1, column: 3 });
        assert_eq!(
            error.inner,
            ParseError::Expecting {
                value: "\"".to_string()
            }
        );
    }

    #[test]
    fn test_bytes_multilines_error() {
        let mut reader = Reader::init("```\nxxx ");
        let error = bytes(&mut reader).err().unwrap();
        assert_eq!(error.pos, Pos { line: 2, column: 5 });
        assert_eq!(
            error.inner,
            ParseError::Expecting {
                value: String::from("```")
            }
        );
    }

    #[test]
    fn test_bytes_eof() {
        let mut reader = Reader::init("");
        let error = bytes(&mut reader).err().unwrap();
        //println!("{:?}", error);
        assert_eq!(
            error.inner,
            ParseError::Expecting {
                value: String::from("file")
            }
        );
        assert_eq!(error.recoverable, true);
    }

    #[test]
    fn test_json_bytes() {
        let mut reader = Reader::init("100");
        assert_eq!(
            json_bytes(&mut reader).unwrap(),
            Bytes::Json {
                value: json::Value::Number("100".to_string())
            }
        );
    }

    #[test]
    fn test_file_bytes() {
        let mut reader = Reader::init("file, filename1;");
        assert_eq!(
            file_bytes(&mut reader).unwrap(),
            Bytes::File {
                space0: Whitespace {
                    value: String::from(" "),
                    source_info: SourceInfo::init(1, 6, 1, 7),
                },
                filename: Filename {
                    value: String::from("filename1"),
                    source_info: SourceInfo::init(1, 7, 1, 16),
                },
                space1: Whitespace {
                    value: String::from(""),
                    source_info: SourceInfo::init(1, 16, 1, 16),
                },
            }
        );

        let mut reader = Reader::init("file, tmp/filename1;");
        assert_eq!(
            file_bytes(&mut reader).unwrap(),
            Bytes::File {
                space0: Whitespace {
                    value: String::from(" "),
                    source_info: SourceInfo::init(1, 6, 1, 7),
                },
                filename: Filename {
                    value: String::from("tmp/filename1"),
                    source_info: SourceInfo::init(1, 7, 1, 20),
                },
                space1: Whitespace {
                    value: String::from(""),
                    source_info: SourceInfo::init(1, 20, 1, 20),
                },
            }
        );
    }

    #[test]
    fn test_file_bytes_error() {
        let mut reader = Reader::init("fil; filename1;");
        let error = file_bytes(&mut reader).err().unwrap();
        assert_eq!(error.pos, Pos { line: 1, column: 1 });
        assert_eq!(error.recoverable, true);

        let mut reader = Reader::init("file, filename1");
        let error = file_bytes(&mut reader).err().unwrap();
        assert_eq!(
            error.pos,
            Pos {
                line: 1,
                column: 16,
            }
        );
        assert_eq!(error.recoverable, false);
        assert_eq!(
            error.inner,
            ParseError::Expecting {
                value: String::from(";")
            }
        );
    }

    #[test]
    fn test_base64_bytes() {
        let mut reader = Reader::init("base64,  T WE=;xxx");
        assert_eq!(
            base64_bytes(&mut reader).unwrap(),
            Bytes::Base64 {
                space0: Whitespace {
                    value: String::from("  "),
                    source_info: SourceInfo::init(1, 8, 1, 10),
                },
                value: vec![77, 97],
                encoded: String::from("T WE="),
                space1: Whitespace {
                    value: String::from(""),
                    source_info: SourceInfo::init(1, 15, 1, 15),
                },
            }
        );
        assert_eq!(reader.state.cursor, 15);
    }
}
