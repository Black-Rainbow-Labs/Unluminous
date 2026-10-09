import React from 'react';

const cx = (...parts) => parts.filter(Boolean).join(' ');

const KEYWORDS = new Set('as async await break const continue crate else enum extern false fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait true type unsafe use where while dyn import export from function class new this null undefined var interface'.split(' '));
const BUILTINS = new Set('String Vec Option Some None Ok Err Result Box usize u8 u16 u32 u64 i32 i64 f32 f64 bool str char println format assert assert_eq console'.split(' '));

/** A Rust file from Unluminous, the default content of an editing pane. */
export const UL_SAMPLE_CODE = `/// Where a name is defined, read from the tokeniser.
pub fn definitions(text: &str, grammar: &Grammar) -> Vec<Definition> {
    let mut found = Vec::new();
    for token in syntax::scan(text, grammar) {
        if let Some(kind) = grammar.definer(token.word) {
            found.push(Definition { kind, at: token.end });
        }
    }
    // 155 files indexed in 38 ms.
    found
}`;

/**
 * Split one line of code into coloured pieces the way a language plugin's grammar would: comments,
 * strings, numbers, keywords, builtins, types and the word before a bracket as a function.
 * @param line - one line of source
 */
export function tokenise(line) {
  const pieces = [];
  const pattern = /(\/\/.*$|#.*$)|("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*')|(\b\d[\d_.]*\b)|([A-Za-z_][A-Za-z0-9_]*)(?=\s*\()|([A-Za-z_][A-Za-z0-9_]*)|(\s+)|([^\sA-Za-z0-9_])/g;
  let match;
  while ((match = pattern.exec(line))) {
    const [text, comment, string, number, call, word] = match;
    let kind = 'text';
    if (comment) kind = 'comment';
    else if (string) kind = 'string';
    else if (number) kind = 'number';
    else if (call) kind = KEYWORDS.has(call) ? 'keyword' : BUILTINS.has(call) ? 'builtin' : 'function';
    else if (word) kind = KEYWORDS.has(word) ? 'keyword' : BUILTINS.has(word) ? 'builtin' : /^[A-Z]/.test(word) ? 'type' : 'text';
    else if (/^[=+\-*/<>!&|:.?]+$/.test(text)) kind = 'operator';
    pieces.push({ kind, text });
  }
  return pieces;
}

/**
 * An editing pane showing code (components::editor_view with components::gutter): line numbers in the
 * gutter, the code coloured by the theme's nine token colours, the caret's line and number picked out.
 * @param code - the text to show
 * @param caretLine - the line the caret is on, from 1
 * @param breakpoints - line numbers with a breakpoint dot drawn over the number
 * @param executionLine - the line a paused program is stopped on
 * @param fontSize - the editor font size, 14 for code
 */
export function UlEditor({ code = UL_SAMPLE_CODE, caretLine = 1, breakpoints = [], executionLine, fontSize = 14, prose = false, className, style, ...rest }) {
  const lines = code.split('\n');
  return (
    <div className={cx('ul-editor', prose && 'ul-editor--prose', className)} style={{ '--ul-editor-size': `${fontSize}px`, ...style }} {...rest}>
      <div className="ul-editor__page">
        {lines.map((line, index) => {
          const number = index + 1;
          return (
            <div key={index} className={cx('ul-editor__line', number === executionLine && 'is-execution')}>
              <span className={cx('ul-editor__number', number === caretLine && 'is-caret')}>
                {breakpoints.includes(number) ? <span className="ul-editor__breakpoint" aria-label={`Breakpoint on line ${number}`} /> : number}
              </span>
              <span className="ul-editor__text">
                {number === caretLine && <span className="ul-editor__caret" />}
                {prose ? line : tokenise(line).map((piece, i) => (
                  <span key={i} className={`ul-syn ul-syn--${piece.kind}`}>{piece.text}</span>
                ))}
                {line === '' && ' '}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
