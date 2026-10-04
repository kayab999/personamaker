'use strict';
/* Minimal DOM shim: just enough to load frontend/script.js in node.
 * The escaper below is INDEPENDENT of the code under test (simple,
 * obviously-correct replacement) so escapeHtml assertions stay meaningful.
 */
function independentEscape(s) {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function makeElement(tag) {
  const el = {
    tagName: String(tag).toUpperCase(),
    children: [],
    style: {},
    dataset: {},
    className: '',
    id: '',
    _text: '',
    _html: '',
    classList: { add() {}, remove() {}, toggle() {}, contains() { return false; } },
    set textContent(v) { this._text = String(v ?? ''); },
    get textContent() { return this._text; },
    set innerHTML(v) { this._html = String(v ?? ''); },
    get innerHTML() {
      // Browser semantics: reading innerHTML serializes escaped text.
      // If _html was set explicitly (template), return as-is; the div used
      // by escapeHtml only ever sets textContent, so return escaped text.
      if (this._html && !this._text) return this._html;
      return independentEscape(this._text);
    },
    appendChild(c) { this.children.push(c); return c; },
    addEventListener() {},
    setAttribute() {},
    querySelector() { return null; },
    querySelectorAll() { return []; },
  };
  return el;
}

const listeners = {};
const documentStub = {
  createElement: (t) => makeElement(t),
  addEventListener: (type, fn) => { (listeners[type] ||= []).push(fn); },
  getElementById: () => null,
  querySelector: () => null,
  querySelectorAll: () => [],
  body: makeElement('body'),
  __listeners: listeners,
};

const store = new Map();
global.window = global;
global.document = documentStub;
global.localStorage = {
  getItem: (k) => (store.has(k) ? store.get(k) : null),
  setItem: (k, v) => store.set(k, String(v)),
  removeItem: (k) => store.delete(k),
};
Object.defineProperty(globalThis, 'navigator', {
  value: { clipboard: { writeText: async () => {} } },
  configurable: true,
  writable: true,
});

module.exports = { documentStub, independentEscape };
