//! Opt-in engraving hints computed at MusicXML write time — a port of MuseScore's
//! automatic engraving rules. Everything here is gated behind
//! [`MusicXmlWriteOptions`](crate::options::MusicXmlWriteOptions); the default writer
//! path never constructs a plan, so its byte-for-byte output is unaffected.

pub(crate) mod beam;
pub(crate) mod stem;
