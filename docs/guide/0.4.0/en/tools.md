# Tools

The only tool in the engine repository is `apidoc`, which extracts API documentation data and checks the documentation. The authoring tools for building UI, particles, timelines and scenes live in the original vanilla.js repository, and the Rust port reads the files those tools save as they are.

## apidoc

A tool that extracts the engine API documentation data (JSON). It is the workspace crate `vanilla-engine-apidoc` and uses the same data format as `tools/apidoc.cjs` in the original vanilla.js. Run it from the engine repository root.

```bash
cargo run -p vanilla-engine-apidoc -- 0.4.0
cargo run -p vanilla-engine-apidoc -- 0.4.0 --check
cargo run -p vanilla-engine-apidoc -- 0.4.0 --missing
cargo run -p vanilla-engine-apidoc -- 0.4.0 --gaps --only src/ui/
```

| Mode | What it does |
| --- | --- |
| (version only) | Reads the public API reachable from outside through `pub mod` and `pub use` in `src/lib.rs` (structs, traits, type aliases, enums, functions, constants, exported macros, the re-exported reflection macro) and writes `docs/api/<version>.json`. `--date YYYY-MM-DD` sets the date, and `--counterpart <vanilla.js data>` rebuilds the table of counterparts in the original |
| `--check` | Checks the shape of the data and prints module, class, member and example counts and the number of empty descriptions. For the guide it checks that ko and en have the same file names and code blocks and that rust code blocks and the en body contain no Hangul. If there are errors the exit code is 1 |
| `--missing` | Prints the description texts missing from the English translation table (`docs/api/translation.en.json`) as a JSON array |
| `--gaps` | Prints, per file, the places in the source where documentation is empty (empty descriptions in the public API, structs and traits without examples, functions without a header comment) |
| `--only`, `--exclude` | Filter the sources `--missing` and `--gaps` look at by the leading part of the path from the repository root. (Can be given several times) |

- Descriptions are read from rustdoc comments. The first paragraph is the description, and the parameter, return and example sections are read separately. Examples are the code blocks in the example section, and the one line of text right before a block is its title.
- Every description field in the data is `{ ko, en }`. ko is the doc comment text as written, and en comes from the translation table. Texts missing from the table leave en empty, and the tool reports how many.
- Unlike the original data, the class slot holds structs, traits and type aliases, told apart by the `kind` field. The same item in the original is written in the `counterpart` field.

## Example and Guide Checks

- API documentation examples are code blocks in doc comments, so `cargo test --doc` compiles and runs them against the engine. Examples that need a window or GL are `no_run` and only compiled.
- The rust code blocks of this guide work the same way. The `vanilla-engine-apidoc` crate builds a doctest for each ko chapter of the highest version. To run one chapter, filter by the joined file name.

```bash
cargo test --doc
cargo test --doc -p vanilla-engine-apidoc
cargo test --doc -p vanilla-engine-apidoc -- Guide08Drawing
```

## Relation to the Original Authoring Tools

The original vanilla.js repository has authoring tools that open in the browser (UI Editor, Particle Editor, Timeline Editor, Visual Editor) and command-line tools run with Node.js (such as `tools/atlas.cjs`). The Rust port has no authoring tools of its own; it reads the files those tools save.

| What an original tool writes | Where the Rust port reads it |
| --- | --- |
| Visual Editor scene files (`.scene.json`), UI Editor documents | `UIDocument` (one call to `register_editor_types` registers every type the original editor registers) |
| Timeline Editor timeline documents | `Timeline` |
| Particle Editor particle descriptions | `ParticleSystem::apply_description` |
| Atlas JSON and page images baked by `tools/atlas.cjs` | `SpriteAtlas` |

File formats per tool and files shared by both engines are in the "Authoring Tools and Files Shared by Both Engines" chapter. The list of original tools and their demos is in the original repository (<https://github.com/ddukbaek2/vanilla.js>).

## Related Types

UIDocument, Timeline, ParticleSystem, SpriteAtlas
