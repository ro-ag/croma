use crate::model::AlignedLyric;

use super::MusicXmlWriter;

impl<'score> MusicXmlWriter<'score> {
    pub(crate) fn write_lyrics(&mut self, lyrics: &[AlignedLyric]) {
        for lyric in lyrics {
            if matches!(
                lyric.control,
                crate::model::LyricControl::Skip | crate::model::LyricControl::Hyphen
            ) {
                continue;
            }
            let number = lyric.verse.to_string();
            self.xml.start("lyric", &[("number", number.as_str())]);
            match lyric.control {
                crate::model::LyricControl::Syllable => {
                    self.xml.text_element("syllabic", "single");
                    self.xml.text_element("text", &lyric.text);
                }
                crate::model::LyricControl::Hyphen => {}
                crate::model::LyricControl::Extender => {
                    self.xml.empty("extend", &[]);
                }
                crate::model::LyricControl::Skip => {}
            }
            self.xml.end("lyric");
        }
    }
}
