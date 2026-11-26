# 바닐라엔진이 고친 판

rustybuzz 0.20.1 (crates.io) 그대로에 아래만 더했다. 고친 자리에는 `vanilla-engine change` 주석이 있다.

- `Face::set_scale`: HarfBuzz `hb_font_set_scale` 처럼 자리 배율을 둔다. 글꼴 단위 정수는 `em_scale`(16.16 곱 `(v * mult + 32768) >> 16`), 가변 GPOS 델타는 `em_scalef`(실수 곱의 반올림)로 그 배율에서 넣는다. 전진폭, 원점, 글리프 범위(`scale_glyph_extents`), GPOS 값과 앵커, kern, kerx, trak, 대체 빈칸과 표시 자리. 기본 배율은 upem 이라 그대로 쓰면 0.20.1 과 같은 값이다.
- `Face::set_variation`, `Face::variation_coordinates`: 축 값을 HarfBuzz `hb_ot_var_normalize_coords` 처럼 정규화해 든다. (16.16 정규화, avar 를 실수로 사상, 2.14 로 반올림) HVAR 전진폭도 그 좌표로 `advance + roundf(delta)` 다.
- ttf-parser 는 옆의 `../ttf-parser` 고친 판을 쓴다.
- 새 rustc 경고 둘(`unstable_name_collisions`, 쓰지 않는 `full`)을 그 자리에서 끈다.

크롬(Blink HarfBuzzFace::GetScaledFont)은 글자 크기의 16.16 고정 소수를 배율로 건다. 바닐라엔진 데스크탑이 크롬 Canvas2D 와 같은 글자 너비를 내려고 고쳤다.

# rustybuzz
![Build Status](https://github.com/harfbuzz/rustybuzz/workflows/Rust/badge.svg)
[![Crates.io](https://img.shields.io/crates/v/rustybuzz.svg)](https://crates.io/crates/rustybuzz)
[![Documentation](https://docs.rs/rustybuzz/badge.svg)](https://docs.rs/rustybuzz)

`rustybuzz` is a complete [harfbuzz](https://github.com/harfbuzz/harfbuzz)'s
shaping algorithm port to Rust.

Matches `harfbuzz` v10.0.1, commit [c7ef6a2e](https://github.com/harfbuzz/harfbuzz/commit/c7ef6a2ed58ae8ec108ee0962bef46f42c73a60c) (one commit after v10.0.1)

## Why?

Because you can add `rustybuzz = "*"` to your project and it just works.
No need for a C++ compiler. No need to configure anything. No need to link to system libraries.

## Conformance

rustybuzz passes nearly all of harfbuzz shaping tests (2221 out of 2252 to be more precise).
So it's mostly identical, but there are still some tiny edge-cases which
are not implemented yet or cannot be implemented at all.

## Major changes

- Subsetting removed.
- TrueType parsing is completely handled by the
  [ttf-parser](https://github.com/RazrFalcon/ttf-parser).
  And while the parsing algorithm is very different, it's not better or worse, just different.
- Malformed fonts will cause an error. HarfBuzz uses fallback/dummy shaper in this case.
- No font size property. Shaping is always using UnitsPerEm. You should scale the result manually.
- Most of the TrueType and Unicode handling code was moved into separate crates.
- rustybuzz doesn't provide any integration with external libraries, so no FreeType, CoreText, or Uniscribe/DirectWrite font-loading integration, and no ICU, or GLib Unicode-functions integration, as well as no `graphite2` library support.
- `mort` table is not supported, since it's deprecated by Apple.
- No Arabic fallback shaper. This requires the ability to build lookups on the fly. In HarfBuzz (C++) this requires serialization code that is associated with subsetting.
- `avar2` as well as other parts of the boring-expansion-spec are not supported yet.

## Performance

At the moment, performance isn't that great. We're 1.5-2x slower than harfbuzz.

See [benches/README.md](./benches/README.md) for details.

## Notes about the port

rustybuzz is not a faithful port.

harfbuzz can roughly be split into 6 parts: shaping, subsetting, TrueType parsing,
Unicode routines, custom containers and utilities (harfbuzz doesn't use C++ std)
and glue for system/3rd party libraries. In the mean time, rustybuzz contains only shaping.
All of the TrueType parsing was moved to the [ttf-parser](https://github.com/RazrFalcon/ttf-parser).
Subsetting was removed. Unicode code was mostly moved to external crates.
We don't need custom containers because Rust's std is good enough.
And we do not use any non Rust libraries, so no glue code either.

In the end, we still have around 23 KLOC. While harfbuzz is around 80 KLOC.

## Lines of code

As mentioned above, rustybuzz has around 23 KLOC. But this is not strictly true,
because there are a lot of auto-generated data tables.

You can find the "real" code size using:

```sh
tokei --exclude hb/unicode_norm.rs --exclude hb/ot_shaper_vowel_constraints.rs \
      --exclude '*_machine.rs' --exclude '*_table.rs' src
```

Which gives us around 17 KLOC, which is still a lot.

## Future work

Since the port is finished, there is not much to do other than syncing it with
a new harfbuzz releases. However, there is still lots of potential areas of improvement:

- **Wider test coverage**: Currently, we only test the result of the positioned glyphs in the shaping output.
We should add tests so that we can test other parts of the API as well, such as glyph extents and glyph flags.
- **Fuzzing against harfbuzz**: While `rustybuzz` passes the whole `harfbuzz` test suite, this does not mean that
output will always be 100% identical to `harfbuzz`. Given the complexity of the code base, there are bound to be
other bugs that just have not been discovered yet. One potential way of addressing this issue could be to create a
fuzzer that takes random fonts, and shapes them with a random set of Unicode codepoints as well as
input settings. In case of a discovered discrepancy, this test case could then be investigated and once the
bug has been identified, added to our custom test suite. On the one hand we could use the Google Fonts font
collection for this so that the fonts can be added to the repository,
but we could also just use MacOS/Windows system fonts and only test them in CI, similarly
to how it's currently done for AAT in `harfbuzz`.
- **Performance**: `harfbuzz` contains tons of optimization structures
(accelerators and caches) which have not been included in `rustybuzz`. As a result of this,
performance is much worse in many cases, as mentioned above (although in the grand scheme of things
`rustybuzz` is still very performant), but the upside of excluding all those optimizations is that
the code base is much simpler and straightforward. This makes it a lot easier to backport new changes
(which already is a very difficult task). Now that we are back in sync with `harfbuzz`, we can consider
attempting to port some of the major optimization improvements from `harfbuzz`, but we should do so carefully
to not make it even harder to keep the code bases in sync.
- **Code alignment**: `rustybuzz` tries its best to make the code base look like a 1:1 C++ to Rust translation,
which is the case for most parts of the code, but there also are many variations (most notable in AAT) that arise
from the fact that a lot of the C++ concepts are not straightforward to port. Nevertheless, there probably still
are parts of the code that probably could be made more similar, given that someone puts time into looking into that.

All of this is a lot of work, so contributions are more than welcome.

## Safety

The library is completely safe.

We do have one `unsafe` to cast between two POD structures, which is perfectly safe.
But except that, there are no `unsafe` in this library and in most of its dependencies
(excluding `bytemuck`).

## Alternatives

- [harfbuzz_rs](https://crates.io/crates/harfbuzz_rs) - bindings to the actual harfbuzz library.
  As of v2 doesn't expose subsetting and glyph outlining, which harfbuzz supports.
- [allsorts](https://github.com/yeslogic/allsorts) - shaper and subsetter.
  As of v0.6 doesn't support variable fonts and
  [Apple Advanced Typography](https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6AATIntro.html).
  Relies on some unsafe code.
- [swash](https://github.com/dfrg/swash) - Supports variable fonts, text layout and rendering.
  No subsetting. Relies on some unsafe code. As of v0.1.4 has zero tests.

## License

`rustybuzz` is licensed under the **MIT**.

`harfbuzz` is [licensed](https://github.com/harfbuzz/harfbuzz/blob/main/COPYING) under the **Old MIT**
