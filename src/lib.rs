use anyhow::Result;
use enum_map::{enum_map, Enum, EnumMap};
use jieba_rs::Jieba;
use miniz_oxide::inflate::decompress_to_vec;
use once_cell::sync::Lazy;
use trie_rs::map::Trie;

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

type Dictionary = Trie<u8, String>;

include!(concat!(env!("OUT_DIR"), "/dicts.rs"));

/// Variant is a source or destination dialect.
#[derive(PartialEq, Eq, Hash, Enum, Clone, Copy)]
pub enum Script {
    /// OpenCC Standard.
    ST,
    /// Simplified Chinese, China.
    CN,
    /// Traditional Chinese, Taiwan.
    TW,
    /// Traditional Chinese, Hong Kong.
    HK,
}

static CONFIGS_TO_STANDARD: Lazy<EnumMap<Script, DictionaryKeys>> = Lazy::new(|| {
    enum_map! {
        Script::ST => DictionaryKeys::FromStandard,
        Script::CN => DictionaryKeys::FromChina,
        Script::TW => DictionaryKeys::FromTaiwan,
        Script::HK => DictionaryKeys::FromHongKong,
    }
});

static CONFIGS_FROM_STANDARD: Lazy<EnumMap<Script, DictionaryKeys>> = Lazy::new(|| {
    enum_map! {
        Script::ST => DictionaryKeys::ToStandard,
        Script::CN => DictionaryKeys::ToChina,
        Script::TW => DictionaryKeys::ToTaiwan,
        Script::HK => DictionaryKeys::ToHongKong,
    }
});

static JIEBA: Lazy<Jieba> = Lazy::new(|| {
    let mut jieba = Jieba::new();
    let key_bytes = decompress_to_vec(include_bytes!(concat!(env!("OUT_DIR"), "/keys.zpostcard")))
        .expect("failed to decompress keys");
    let keys: Vec<String> = postcard::from_bytes(&key_bytes).expect("failed to load extra words");
    for key in keys {
        jieba.add_word(key.as_str(), None, None);
    }
    jieba
});

/// Convert a single word.
fn convert_word<'a>(keys: impl Iterator<Item = &'a DictionaryKeys>, input: &str) -> Result<String> {
    let mut word = input.to_owned();
    for key in keys {
        let mut parts = Vec::new();
        let dict = &DICTIONARIES[*key];
        let mut offset = 0;
        while offset < word.len() {
            let result: Option<(String, &String)> =
                dict.common_prefix_search(&word[offset..]).last();
            match result {
                Some((matched, value)) => {
                    parts.push(value.to_owned());
                    offset += matched.len();
                }
                None => {
                    match word[offset..].chars().next() {
                        Some(ch) => {
                            let len = ch.len_utf8();
                            parts.push(word[offset..offset + len].to_owned());
                            offset += len;
                        }
                        None => {
                            parts.push(word[offset..].to_owned());
                            offset += word[offset..].len();
                        }
                    };
                }
            }
        }
        word = parts.join("");
    }
    Ok(word)
}

/// Convert a string from an input variant to an output variant.
pub fn convert(from: Script, to: Script, input: &str) -> Result<Vec<String>> {
    let all_words = JIEBA.cut(input, true);
    let words = all_words.iter().cloned();
    let keys = [CONFIGS_TO_STANDARD[from], CONFIGS_FROM_STANDARD[to]];
    let result = words.filter_map(move |word| convert_word(keys.iter(), word).ok());

    Ok(result.collect())
}

#[cfg(feature = "wasm")]
pub struct JSError {
    val: String,
}

#[cfg(feature = "wasm")]
impl Into<JsValue> for JSError {
    fn into(self) -> JsValue {
        JsValue::from_str(self.val.as_str())
    }
}

#[cfg(feature = "wasm")]
impl<T: ToString> From<T> for JSError {
    fn from(value: T) -> Self {
        JSError {
            val: value.to_string(),
        }
    }
}

#[cfg(feature = "wasm")]
#[wasm_bindgen(js_name = convert)]
pub fn convert_export(from: &str, to: &str, input: &str) -> std::result::Result<String, JSError> {
    let from_script = match from {
        "cn" => Script::CN,
        "tw" => Script::TW,
        "hk" => Script::HK,
        _ => return Err(format!("invalid from script {}", from).into()),
    };
    let to_script = match to {
        "cn" => Script::CN,
        "tw" => Script::TW,
        "hk" => Script::HK,
        _ => return Err(format!("invalid to script {}", to).into()),
    };
    Ok(convert(from_script, to_script, input)?.join(""))
}

#[cfg(test)]
mod tests {
    use std::{env, fs, path};

    use super::*;

    #[test]
    fn test_convert_word() -> Result<()> {
        let keys = vec![DictionaryKeys::FromChina];
        let result = convert_word(keys.iter(), "㐷")?;
        assert_eq!("傌", result);

        Ok(())
    }

    #[test]
    fn test_convert_word_hk_rev() -> Result<()> {
        let keys = vec![DictionaryKeys::FromHongKong];
        let result = convert_word(keys.iter(), "吃")?;
        assert_eq!("喫", result);

        Ok(())
    }

    mod phrase_tests {
        use super::*;

        macro_rules! parameterized_test {
            ($name:ident, $from:expr, $to:expr, $input:expr, $expected:expr) => {
                #[test]
                fn $name() -> Result<()> {
                    let result = convert($from, $to, $input)?;
                    assert_eq!($expected, result.join(""));

                    Ok(())
                }
            };
        }

        parameterized_test!(simple, Script::ST, Script::TW, "優化", "最佳化");
        // This one fails, see https://github.com/BYVoid/OpenCC/issues/848
        // parameterized_test!(alphabet, Variant::TW, Variant::CN, "英文字母", "英文字母");
        parameterized_test!(
            opencc664,
            Script::CN,
            Script::TW,
            "他们是勇敢的士兵",
            "他們是勇敢的士兵"
        );
    }

    mod opencc_tests {
        use super::*;
        macro_rules! parameterized_test {
            ($name:ident, $from:expr, $to:expr) => {
                #[test]
                fn $name() -> Result<()> {
                    let cases_dir = path::Path::new(&env::var("CARGO_MANIFEST_DIR")?)
                        .join("opencc/test/testcases");
                    let input_path = cases_dir.join(format!("{0}.in", stringify!($name)));
                    let expected_path = cases_dir.join(format!("{0}.ans", stringify!($name)));
                    let input = fs::read_to_string(input_path)?;
                    let expected = fs::read_to_string(expected_path)?;
                    let result = convert($from, $to, &input)?.join("");
                    assert_eq!(
                        expected,
                        result,
                        "Mismatch with test case {}",
                        stringify!($name)
                    );
                    Ok(())
                }
            };
        }

        parameterized_test!(hk2s, Script::HK, Script::CN);
        parameterized_test!(hk2t, Script::HK, Script::ST);
        parameterized_test!(s2hk, Script::CN, Script::HK);
        parameterized_test!(s2t, Script::CN, Script::ST);
        parameterized_test!(s2tw, Script::CN, Script::TW);
        parameterized_test!(s2twp, Script::CN, Script::TW);
        parameterized_test!(t2hk, Script::ST, Script::HK);
        parameterized_test!(t2s, Script::ST, Script::CN);
        parameterized_test!(tw2s, Script::TW, Script::CN);
        parameterized_test!(tw2sp, Script::TW, Script::CN);
        parameterized_test!(tw2t, Script::TW, Script::ST);
    }
}
