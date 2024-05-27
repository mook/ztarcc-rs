# ztarcc-rs

This is a badly done port of [StarCC]; includes both a library and a CLI that
uses it.  There is also an [online demo] using WebAssembly (sadly nearly 10MB).

[StarCC]: https://github.com/StarCC0/starcc-py
[online demo]: https://mook.github.io/ztarcc-rs/

### Notes on the library:

- All of the dictionary data is embedded into the library directly; there
  are no external files to load.
- OpenCC is set up as a submodule, and the dictionaries are generated at
  build time from the files in there.
- Like StarCC, Jieba is always used.  Here we assume HMM is wanted.
- The library API should be using iterators but isn't yet.
- See GitHub Actions [workflow] for compiling to WebAssembly.

[workflow]: .github/workflows/pages.yaml

### Notes on the CLI:

- To build the CLI, use `cargo build --features cli`.
- The input may be on standard in or a file; similarly, the output may be
  standard out or a file.
- Input encoding is auto-detected among the likely Chinese encodings; the
  output is always UTF-8.
- We always read all of the input into memory before working on it.  This
  may need to be improved later.
- Conversion is parallelized on lines.
