use anyhow::{anyhow, Context, Result};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};
use std::{env, fs, io, path};
use trie_rs::map::TrieBuilder;

/// Read a dictionary from disk.
fn read_dict(in_dir: &path::Path, name: &str) -> Result<HashMap<String, String>> {
    let mut in_path = in_dir.join(name);
    in_path.set_extension("txt");
    let in_file = fs::File::open(in_path).context(format!("reading dictionary {0}", name))?;

    let mut map = HashMap::<String, String>::new();
    for maybe_line in io::BufReader::new(in_file).lines() {
        let line = maybe_line?;
        let (from, rest) = line
            .split_once('\t')
            .ok_or(anyhow!("could not split line"))?;
        if let Some(first_token) = rest.split_ascii_whitespace().next() {
            map.insert(from.to_owned(), first_token.to_owned());
        }
    }
    Ok(map)
}

/// Reverse a dictionary.
fn reverse_dict(in_dict: &HashMap<String, String>) -> HashMap<String, String> {
    HashMap::from_iter(in_dict.iter().map(|(k, v)| (v.to_owned(), k.to_owned())))
}

/// Reads all dictionary files in OpenCC, generating a serialized trie for each.
/// Emitted files are placed in `$OUT_DIR` with a `.postcard` extension.
/// Also emits a `keys.postcard` with all keys.
/// Returns the list of dictionaries.
fn build_all_dicts(out_dir: &path::Path) -> Result<Vec<String>> {
    let dict_definitions = HashMap::from([
        ("FromStandard", vec![]),
        ("FromChina", vec!["STCharacters", "STPhrases"]),
        (
            "FromTaiwan",
            vec![
                "!TWVariants",
                "TWVariantsRevPhrases",
                "!TWPhrasesIT",
                "!TWPhrasesName",
                "!TWPhrasesOther",
            ],
        ),
        ("FromHongKong", vec!["!HKVariants", "HKVariantsRevPhrases"]),
        ("ToStandard", vec![]),
        ("ToChina", vec!["TSCharacters", "TSPhrases"]),
        (
            "ToTaiwan",
            vec![
                "TWVariants",
                "TWPhrasesIT",
                "TWPhrasesName",
                "TWPhrasesOther",
            ],
        ),
        ("ToHongKong", vec!["HKVariants"]),
    ]);
    let source_dir = fs::canonicalize(
        path::Path::new(&env::var("CARGO_MANIFEST_DIR")?).join("opencc/data/dictionary"),
    )?;
    println!("cargo::rerun-if-changed={0}", source_dir.display());

    let names: Vec<_> = dict_definitions
        .values()
        .flatten()
        .map(|v| v.trim_start_matches('!'))
        .collect();
    let mut dicts: HashMap<&str, HashMap<String, String>> =
        HashMap::from_iter(names.iter().map(|name| {
            let dict = read_dict(&source_dir, name)
                .context(anyhow!(format!("failed to read {}", name)))
                .unwrap();
            (*name, dict)
        }));

    // The largest dictionary by far is STPhrases, which is never used in reverse; therefore, we can
    // optimize total time by doing the reverse ahead of time so that we don't need to clone the huge dict.
    for dict in dict_definitions.values().flatten() {
        if let ("!", without_prefix) = dict.split_at(1) {
            dicts.insert(
                dict,
                reverse_dict(
                    dicts
                        .get(without_prefix)
                        .ok_or(anyhow!(format!("failed to find dict {}", dict)))?,
                ),
            );
        }
    }

    let all_keys: Vec<_> = dict_definitions
        .iter()
        .map(|(out_name, in_names)| -> Result<HashSet<String>> {
            let mut keys = HashSet::<String>::new();
            let mut builder = TrieBuilder::<u8, String>::new();
            for in_name in in_names {
                let from_dict = dicts.get(in_name).ok_or(anyhow!(format!(
                    "failed to find dictionary {} while constructing {}",
                    in_name, out_name
                )))?;
                from_dict
                    .iter()
                    .for_each(|(k, v)| builder.push(k, v.to_owned()));
                keys.extend(
                    from_dict
                        .keys()
                        .filter(|k| k.len() > 3)
                        .map(|v| v.to_string()),
                );
            }
            let mut out_path = out_dir.join(out_name);
            out_path.set_extension("postcard");
            let out_file = fs::File::create(out_path).context(format!(
                "could not open dictionary output for {0}",
                out_name
            ))?;
            let dict = builder.build();
            postcard::to_io(&dict, out_file)
                .context(format!("writing dictionary data {0}", out_name))?;

            Ok(keys)
        })
        .collect();
    let mut merged_keys = HashSet::<String>::new();
    for keys in all_keys {
        merged_keys.extend(keys?);
    }
    let keys_path = out_dir.join("keys.postcard");
    let keys_file = fs::File::create(keys_path).context(anyhow!("could not open keys output"))?;
    postcard::to_io(&merged_keys, keys_file)?;

    Ok(dict_definitions.keys().map(|k| k.to_string()).collect())
}

/// Write out the main source file that will be included in the library.
fn write_source(out_dir: &path::Path, names: &Vec<String>) -> Result<()> {
    let out_path = out_dir.join("dicts.rs");
    let mut out_file = fs::File::create(out_path)?;

    writeln!(
        out_file,
        "#[derive(PartialEq,Eq,Hash,Debug,Clone,Copy,enum_map::Enum)]"
    )?;
    writeln!(
        out_file,
        "/// DictionaryKeys lists the available dictionary types"
    )?;
    writeln!(out_file, "enum DictionaryKeys {{")?;
    for name in names {
        writeln!(out_file, "  {0},", name)?;
    }
    writeln!(out_file, "}}")?;
    writeln!(
        out_file,
        "type Dictionaries = enum_map::EnumMap<DictionaryKeys, Dictionary>;"
    )?;
    writeln!(out_file)?;

    writeln!(out_file, "static DICTIONARIES: once_cell::sync::Lazy<Dictionaries> = once_cell::sync::Lazy::new(|| {{ enum_map::enum_map! {{")?;
    for name in names {
        write!(
            out_file,
            "  DictionaryKeys::{0} => postcard::from_bytes(",
            name
        )?;
        writeln!(out_file, "include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{0}.postcard\"))).expect(\"failed to load dictionary {0}\"),", name)?;
    }
    writeln!(out_file, "}}}});")?;

    Ok(())
}

/// Build everything.
fn build_all() -> Result<()> {
    let out_dir = fs::canonicalize(path::Path::new(&env::var("OUT_DIR")?))?;
    let names = build_all_dicts(&out_dir)?;

    write_source(&out_dir, &names)?;
    println!(
        "cargo::warning=Generated code written to {0}",
        out_dir.display()
    );
    Ok(())
}

fn main() {
    build_all().unwrap();
}
