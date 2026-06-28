use crate::model::{AlignedLyric, LyricControl};

use super::{MusicXmlWriter, OpenLyricHyphen};

impl<'score> MusicXmlWriter<'score> {
    pub(crate) fn write_lyrics(&mut self, lyrics: &[AlignedLyric], voice_key: &str) {
        for lyric in lyrics {
            if matches!(lyric.control, LyricControl::Skip | LyricControl::Hyphen) {
                continue;
            }
            let number = lyric.verse.to_string();
            self.xml.start("lyric", &[("number", number.as_str())]);
            match lyric.control {
                LyricControl::Syllable => {
                    let syllabic = self.syllabic_for_lyric(lyric, voice_key, lyrics);
                    self.xml.text_element("syllabic", syllabic);
                    self.xml.text_element("text", &lyric.text);
                    if lyric.same_note_extend {
                        self.xml.empty("extend", &[]);
                    }
                }
                LyricControl::Hyphen => {}
                LyricControl::Extender => {
                    self.xml.empty("extend", &[]);
                }
                LyricControl::Skip => {}
            }
            self.xml.end("lyric");
        }
    }

    fn syllabic_for_lyric(
        &mut self,
        lyric: &AlignedLyric,
        voice_key: &str,
        note_lyrics: &[AlignedLyric],
    ) -> &'static str {
        let open_index = self
            .lyric_hyphen_open
            .iter()
            .position(|open| open.voice_key == voice_key && open.verse == lyric.verse);
        let follows_hyphen = open_index.is_some();
        let continues = note_lyrics.iter().any(|candidate| {
            candidate.verse == lyric.verse && candidate.control == LyricControl::Hyphen
        });

        match (follows_hyphen, continues) {
            (false, false) => "single",
            (false, true) => {
                self.lyric_hyphen_open.push(OpenLyricHyphen {
                    voice_key: voice_key.to_owned(),
                    verse: lyric.verse,
                });
                "begin"
            }
            (true, true) => "middle",
            (true, false) => {
                if let Some(index) = open_index {
                    self.lyric_hyphen_open.remove(index);
                }
                "end"
            }
        }
    }
}
