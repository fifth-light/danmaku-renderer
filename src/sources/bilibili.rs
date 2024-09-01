use core::str;
use std::{
    cmp::Ordering,
    error::Error,
    fmt::Display,
    io::{BufRead, Cursor},
    num::{ParseFloatError, ParseIntError},
    path::Path,
    str::Utf8Error,
};

use prost::Message;
use quick_xml::{
    events::{attributes::AttrError, Event},
    reader::Reader,
};

use super::{DanmakuSource, VecDanmakuSource};

use crate::danmaku::{Danmaku, DanmakuColor, DanmakuSize, DanmakuTime, DanmakuType};

#[derive(Debug, PartialEq, Eq)]
enum BilibiliXmlReaderState {
    OutOfRoot,
    InsideOfRootNode,
    InsideOfMetadataNode,
    InsideOfDanmakuNode,
    Eof,
}

#[derive(Debug)]
pub enum BilibiliXmlParseError {
    InvalidRootNode(String),
    UnknownNode(String),
    FoundDuplicateAttributes,
    MissingAttributes,
    BadAttribute,
    XmlReadError(quick_xml::Error),
    InvalidXmlAttribute(quick_xml::events::attributes::AttrError),
    InvalidInteger(ParseIntError),
    InvalidFloat(ParseFloatError),
    InvalidUtf8(Utf8Error),
}

impl Display for BilibiliXmlParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRootNode(name) => write!(f, "Invalid root node: {}", name),
            Self::UnknownNode(name) => write!(f, "Unknown node: {}", name),
            Self::FoundDuplicateAttributes => write!(f, "Found duplicate attributes"),
            Self::MissingAttributes => write!(f, "Missing attributes"),
            Self::BadAttribute => write!(f, "Bad attributes"),
            Self::XmlReadError(err) => write!(f, "Failed to parse XML: {}", err),
            Self::InvalidXmlAttribute(err) => write!(f, "Invalid XML Attribute: {}", err),
            Self::InvalidInteger(err) => write!(f, "Invalid integer: {}", err),
            Self::InvalidFloat(err) => write!(f, "Invalid float: {}", err),
            Self::InvalidUtf8(err) => write!(f, "Invalid UTF-8 in XML: {}", err),
        }
    }
}

impl Error for BilibiliXmlParseError {}

impl From<quick_xml::Error> for BilibiliXmlParseError {
    fn from(value: quick_xml::Error) -> Self {
        BilibiliXmlParseError::XmlReadError(value)
    }
}

impl From<AttrError> for BilibiliXmlParseError {
    fn from(value: AttrError) -> Self {
        BilibiliXmlParseError::InvalidXmlAttribute(value)
    }
}

impl From<ParseFloatError> for BilibiliXmlParseError {
    fn from(value: ParseFloatError) -> Self {
        BilibiliXmlParseError::InvalidFloat(value)
    }
}

impl From<ParseIntError> for BilibiliXmlParseError {
    fn from(value: ParseIntError) -> Self {
        BilibiliXmlParseError::InvalidInteger(value)
    }
}

impl From<Utf8Error> for BilibiliXmlParseError {
    fn from(value: Utf8Error) -> Self {
        BilibiliXmlParseError::InvalidUtf8(value)
    }
}

fn parse_xml<R: BufRead>(
    mut reader: Reader<R>,
) -> Result<impl DanmakuSource, BilibiliXmlParseError> {
    let mut buf = Vec::new();
    let mut depth: u32 = 0;
    let mut state = BilibiliXmlReaderState::OutOfRoot;

    let mut result = Vec::new();
    let mut attributes: Option<String> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(start)) => {
                match state {
                    BilibiliXmlReaderState::OutOfRoot => {
                        assert_eq!(depth, 0);
                        state = match start.name().0 {
                            b"i" => BilibiliXmlReaderState::InsideOfRootNode,
                            name => {
                                let name = str::from_utf8(name)?.to_string();
                                return Err(BilibiliXmlParseError::InvalidRootNode(name));
                            }
                        };
                    }
                    BilibiliXmlReaderState::InsideOfRootNode => {
                        assert_eq!(depth, 1);
                        state = match start.name().0 {
                            b"d" => {
                                for item in start.attributes() {
                                    let item = item?;
                                    if item.key.0 == b"p" {
                                        if attributes.is_some() {
                                            return Err(
                                                BilibiliXmlParseError::FoundDuplicateAttributes,
                                            );
                                        }
                                        let value = str::from_utf8(&item.value)?;
                                        attributes = Some(value.to_string())
                                    }
                                }
                                BilibiliXmlReaderState::InsideOfDanmakuNode
                            }
                            _ => BilibiliXmlReaderState::InsideOfMetadataNode,
                        };
                    }
                    BilibiliXmlReaderState::InsideOfMetadataNode => (),
                    BilibiliXmlReaderState::InsideOfDanmakuNode | BilibiliXmlReaderState::Eof => {
                        let name = str::from_utf8(start.name().0)?.to_string();
                        return Err(BilibiliXmlParseError::UnknownNode(name));
                    }
                }
                depth = depth.checked_add(1).expect("Depth overflow");
            }
            Ok(Event::End(end)) => {
                match state {
                    BilibiliXmlReaderState::OutOfRoot => unreachable!(),
                    BilibiliXmlReaderState::InsideOfRootNode => {
                        assert_eq!(depth, 1);
                        assert_eq!(end.name().0, b"i");
                        state = BilibiliXmlReaderState::Eof;
                    }
                    BilibiliXmlReaderState::InsideOfMetadataNode => match depth.cmp(&2) {
                        Ordering::Less => unreachable!(),
                        Ordering::Equal => {
                            state = BilibiliXmlReaderState::InsideOfRootNode;
                        }
                        Ordering::Greater => (),
                    },
                    BilibiliXmlReaderState::InsideOfDanmakuNode => {
                        assert_eq!(depth, 2);
                        assert_eq!(end.name().0, b"d");
                        state = BilibiliXmlReaderState::InsideOfRootNode;
                    }
                    BilibiliXmlReaderState::Eof => panic!("End of unknown node"),
                };
                depth = depth.checked_sub(1).expect("Depth underflow");
            }
            Ok(Event::Text(evt)) => {
                if state == BilibiliXmlReaderState::InsideOfDanmakuNode {
                    assert_eq!(depth, 2);
                    let text = evt.unescape()?.into_owned();
                    let attributes = attributes
                        .take()
                        .ok_or(BilibiliXmlParseError::MissingAttributes)?;
                    let mut time: Option<DanmakuTime> = None;
                    let mut r#type: Option<DanmakuType> = None;
                    let mut size: Option<DanmakuSize> = None;
                    let mut color: Option<DanmakuColor> = None;
                    for (i, item) in attributes.split(',').enumerate() {
                        match i {
                            0 => {
                                let seconds: f64 = item.parse()?;
                                time = Some(DanmakuTime::from_millis((seconds * 1000.0) as u32));
                            }
                            1 => {
                                let num: u32 = item.parse()?;
                                r#type = Some(match num {
                                    1..=3 => DanmakuType::Scroll,
                                    4 => DanmakuType::Bottom,
                                    5 => DanmakuType::Top,
                                    _ => DanmakuType::Unknown,
                                });
                            }
                            2 => {
                                let num: u32 = item.parse()?;
                                size = Some(match num.cmp(&25) {
                                    Ordering::Less => DanmakuSize::Small,
                                    Ordering::Equal => DanmakuSize::Regular,
                                    Ordering::Greater => DanmakuSize::Large,
                                });
                            }
                            3 => {
                                let code: u32 = item.parse()?;
                                color = Some(DanmakuColor::from_code_cast(code));
                            }
                            _ => break,
                        }
                    }
                    let time = time.ok_or(BilibiliXmlParseError::BadAttribute)?;
                    let r#type = r#type.ok_or(BilibiliXmlParseError::BadAttribute)?;
                    let size = size.ok_or(BilibiliXmlParseError::BadAttribute)?;
                    let color = color.ok_or(BilibiliXmlParseError::BadAttribute)?;
                    let danmaku = Danmaku {
                        time,
                        color,
                        size,
                        r#type,
                        content: text,
                    };
                    result.push(danmaku);
                }
            }
            Ok(Event::Eof) => break,
            Err(ex) => return Err(BilibiliXmlParseError::XmlReadError(ex)),
            _ => (),
        }
    }
    Ok(VecDanmakuSource::new(result))
}

pub fn parse_xml_from_file<P: AsRef<Path>>(
    path: P,
) -> Result<impl DanmakuSource, BilibiliXmlParseError> {
    let reader = Reader::from_file(path)?;
    parse_xml(reader)
}

pub fn parse_xml_from_reader<R: BufRead>(
    reader: R,
) -> Result<impl DanmakuSource, BilibiliXmlParseError> {
    let reader = Reader::from_reader(reader);
    parse_xml(reader)
}

#[allow(clippy::all)]
mod bilibili {
    pub mod community {
        pub mod service {
            pub mod dm {
                pub mod v1 {
                    include!(concat!(
                        env!("OUT_DIR"),
                        "/bilibili.community.service.dm.v1.rs"
                    ));
                }
            }
        }
    }
}

use bilibili::community::service::dm::v1::DmSegMobileReply;

pub fn parse_proto(buf: &[u8]) -> Result<impl DanmakuSource, Box<dyn Error>> {
    let mut cursor = Cursor::new(buf);
    let message = DmSegMobileReply::decode(&mut cursor)?;
    let vec: Vec<Danmaku> = message
        .elems
        .into_iter()
        .map(|item| Danmaku {
            time: DanmakuTime::from_millis(item.progress.max(0) as u32),
            r#type: match item.mode {
                1..=3 => DanmakuType::Scroll,
                4 => DanmakuType::Bottom,
                5 => DanmakuType::Top,
                _ => DanmakuType::Unknown,
            },
            size: match item.fontsize.cmp(&25) {
                Ordering::Less => DanmakuSize::Small,
                Ordering::Equal => DanmakuSize::Regular,
                Ordering::Greater => DanmakuSize::Large,
            },
            color: DanmakuColor::from_code_cast(item.color),
            content: item.content,
        })
        .collect();
    Ok(VecDanmakuSource::new(vec))
}

#[cfg(test)]
mod test {
    use std::{fs::File, io::Read, path::Path};

    use crate::{
        danmaku::{DanmakuColor, DanmakuSize, DanmakuTime, DanmakuType},
        sources::{
            bilibili::{parse_proto, parse_xml_from_file},
            DanmakuSource,
        },
    };

    #[test]
    fn test_read_xml() {
        let path = Path::new("test/747529524.xml");
        let mut source = parse_xml_from_file(path).unwrap();

        let mut iter = source.get_all();

        let item = iter.next().unwrap();
        assert_eq!(item.time, DanmakuTime::from_millis(12139));
        assert_eq!(item.r#type, DanmakuType::Scroll);
        assert_eq!(item.size, DanmakuSize::Regular);
        assert_eq!(item.color, DanmakuColor::from_code(0xFFFFFF));
        assert_eq!(item.content, "kksk");

        let item = iter.next().unwrap();
        assert_eq!(item.time, DanmakuTime::from_millis(83679));
        assert_eq!(item.r#type, DanmakuType::Scroll);
        assert_eq!(item.size, DanmakuSize::Regular);
        assert_eq!(item.color, DanmakuColor::from_code(0xFFFFFF));
        assert_eq!(item.content, "喜欢这段的吉他");

        let item = iter.next();
        assert!(item.is_none());
    }

    #[test]
    fn test_read_large_xml() {
        let path = Path::new("test/1176840.xml");
        let mut source = parse_xml_from_file(path).unwrap();

        let iter = source.get_all();

        for item in iter {
            println!("{:?} ({:?}): {}", item.time, item.color, item.content);
        }
    }

    #[test]
    fn test_read_protobuf() {
        let mut file = File::open("test/747529524.bin").unwrap();
        let mut content = Vec::new();
        file.read_to_end(&mut content).unwrap();
        let mut reader = parse_proto(&content).unwrap();

        let mut iter = reader.get_all();

        let item = iter.next().unwrap();
        assert_eq!(item.time, DanmakuTime::from_millis(83679));
        assert_eq!(item.r#type, DanmakuType::Scroll);
        assert_eq!(item.size, DanmakuSize::Regular);
        assert_eq!(item.color, DanmakuColor::from_code(0xFFFFFF));
        assert_eq!(item.content, "喜欢这段的吉他");

        let item = iter.next();
        assert!(item.is_none());
    }

    #[test]
    fn test_read_large_protobuf() {
        let mut file = File::open("test/1176840.bin").unwrap();
        let mut content = Vec::new();
        file.read_to_end(&mut content).unwrap();
        let mut reader = parse_proto(&content).unwrap();

        for item in reader.get_all() {
            println!("{:?} ({:?}): {}", item.time, item.color, item.content);
        }
    }
}
