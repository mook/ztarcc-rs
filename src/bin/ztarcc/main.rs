use anyhow::{anyhow, Result};
use clap::{builder::PossibleValue, Parser, ValueEnum};
use encoding_rs::{BIG5, GB18030, UTF_8};
use rayon::prelude::*;
use std::{
    fs,
    io::{self, BufWriter, Read, Write},
};

#[derive(Clone, Debug)]
enum Script {
    /// Convert from or to Simplified Chinese.
    Simplified,
    /// Convert from or to Traditional Chinese (Taiwan).
    Taiwan,
    /// Convert from or to Traditional Chinese (Hong Kong).
    HongKong,
}

impl Default for Script {
    fn default() -> Self {
        Self::Simplified
    }
}

impl ValueEnum for Script {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Simplified, Self::Taiwan, Self::HongKong]
    }
    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(match self {
            Self::Simplified => PossibleValue::new("cn"),
            Self::Taiwan => PossibleValue::new("tw"),
            Self::HongKong => PossibleValue::new("hk"),
        })
    }
}

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// The input file to convert.  Use "-" to read from standard in.
    #[arg(default_value = "-")]
    input: String,

    /// The output file.  Use "-" to print to standard output.
    #[arg(default_value = "-")]
    output: String,

    /// The input script.
    #[arg(short, long, value_enum, default_value = "cn")]
    from: Script,

    /// The output script.
    #[arg(short, long, value_enum, default_value = "tw")]
    to: Script,
}

fn setup() -> Result<()> {
    let args = Args::parse();
    let mut input = Vec::new();
    match args.input.as_str() {
        "-" => io::stdin().read_to_end(&mut input)?,
        _ => fs::File::open(args.input)?.read_to_end(&mut input)?,
    };
    let mut output: Box<dyn Write> = match args.output.as_str() {
        "-" => Box::new(io::stdout()),
        _ => Box::new(BufWriter::new(fs::File::create(args.output)?)),
    };
    let from_script = match args.from {
        Script::Simplified => ztarcc_rs::Script::CN,
        Script::Taiwan => ztarcc_rs::Script::TW,
        Script::HongKong => ztarcc_rs::Script::HK,
    };
    let to_script = match args.to {
        Script::Simplified => ztarcc_rs::Script::CN,
        Script::Taiwan => ztarcc_rs::Script::TW,
        Script::HongKong => ztarcc_rs::Script::HK,
    };
    let mut detect_settings = charset_normalizer_rs::entity::NormalizerSettings::default().clone();
    detect_settings.include_encodings =
        vec!["utf-8".to_owned(), "big5".to_owned(), "gb18030".to_owned()];
    let encoding_matches = charset_normalizer_rs::from_bytes(&input, Some(detect_settings));
    let encoding = encoding_matches
        .get_best()
        .ok_or(anyhow!(format!("Failed to detect source encoding")))?
        .encoding();
    let (decoded, _, _) = match encoding {
        "utf-8" => UTF_8.decode(&input),
        "big5" => BIG5.decode(&input),
        "gb18030" => GB18030.decode(&input),
        _ => return Err(anyhow!(format!("Failed to decode from {}", encoding))),
    };
    let lines: Vec<_> = decoded
        .split_inclusive('\n')
        .collect::<Vec<_>>()
        .par_iter()
        .map(|line| ztarcc_rs::convert(from_script, to_script, line))
        .collect();

    for line in lines {
        for chunk in line? {
            output.write_all(chunk.as_bytes())?;
        }
    }
    Ok(())
}

fn main() {
    setup().unwrap();
}
