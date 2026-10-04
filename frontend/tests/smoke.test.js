'use strict';
/* Sweep W2: frontend smoke seed (F-12). Zero dependencies — node:test only.
 * Loads the real frontend/script.js under a minimal DOM shim and asserts the
 * P0-2 security properties hold: escaping, avatar-color allowlist, no inline
 * handlers in rendered output, delegated fallback attributes present.
 */
const { describe, it } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

require('./shim.js');

const scriptPath = path.join(__dirname, '..', 'script.js');
vm.runInThisContext(fs.readFileSync(scriptPath, 'utf8'), { filename: 'script.js' });

const { escapeHtml, sanitizeAvatarColor, formatMessageContent, renderSingleMessage } = globalThis;
for (const [name, fn] of Object.entries({ escapeHtml, sanitizeAvatarColor, formatMessageContent, renderSingleMessage })) {
  assert.equal(typeof fn, 'function', `${name} must be loadable from script.js`);
}

const hostileChar = {
  id: 'evil-1',
  name: `'"<img src=x onerror=alert(1)>`,
  avatarUrl: 'https://evil.example.com/a.png',
  avatarColor: 'red;}</style><script>alert(1)</script>',
};

const hostileMsg = {
  id: 'm1',
  role: 'assistant',
  content: `Hello <script>alert('xss')</script> **bold** <img src=x onerror=alert(2)>`,
  timestamp: Date.now(),
  speaker_name: `<b>spoof</b>`,
};

describe('escapeHtml', () => {
  it('neutralizes markup', () => {
    const out = escapeHtml(`<script>alert(1)</script>`);
    assert.ok(!out.includes('<script>'), `raw tag leaked: ${out}`);
    assert.ok(out.includes('&lt;script&gt;'), `expected escaping: ${out}`);
  });
});

describe('sanitizeAvatarColor', () => {
  it('keeps valid hex, resets hostile input', () => {
    assert.equal(sanitizeAvatarColor('#7c3aed'), '#7c3aed');
    assert.equal(sanitizeAvatarColor(hostileChar.avatarColor), '#7c3aed');
  });
});

describe('formatMessageContent', () => {
  it('renders markdown subset without passing through HTML', () => {
    const out = formatMessageContent(`**hi** <script>alert(1)</script>`);
    assert.ok(out.includes('<strong>hi</strong>'), `markdown lost: ${out}`);
    assert.ok(!out.includes('<script>'), `raw HTML leaked: ${out}`);
  });
});

// Strips &lt;...&gt; spans (escaped text content) so assertions only see
// REAL markup — an escaped "onerror=" inside message text is harmless.
function markupOnly(html) {
  return html.replace(/&lt;.*?&gt;/g, '');
}

describe('renderSingleMessage (hostile character + content)', () => {
  it('emits no inline handlers and escapes everything', () => {
    const html = renderSingleMessage(hostileMsg, hostileChar);
    const markup = markupOnly(html);
    assert.ok(!markup.includes('onerror='), `inline handler in output: ${html}`);
    assert.ok(!markup.includes('onclick='), `inline handler in output: ${html}`);
    assert.ok(!html.includes('<script>'), `raw script tag in output: ${html}`);
    assert.ok(!html.includes('<b>spoof</b>'), `raw speaker_name in output: ${html}`);
    // Avatar color must be the allowlisted default, never the payload.
    assert.ok(!html.includes('red;}'), `CSS injection in output: ${html}`);
  });

  it('neutralizes quote-breakout in URL attributes', () => {
    // escapeHtmlAttr is what guards every src="/data-*=" interpolation.
    const { escapeHtmlAttr } = globalThis;
    assert.equal(typeof escapeHtmlAttr, 'function', 'escapeHtmlAttr loadable');
    const out = escapeHtmlAttr(`x" onerror="alert(1)`);
    assert.ok(!out.includes('"'), `raw quote leaked: ${out}`);
    assert.ok(out.includes('&quot;'), `quotes must be entity-encoded: ${out}`);
    // Static backstop: no raw escapeHtml() left inside a quoted attribute.
    const src = fs.readFileSync(scriptPath, 'utf8');
    assert.ok(!src.includes('src="${escapeHtml('), 'raw escapeHtml in src= attribute');
    assert.ok(
      !src.includes('data-avatar-path="${escapeHtml('),
      'raw escapeHtml in data-avatar-path'
    );
  });

  it('wires delegated avatar fallback, not inline JS', () => {
    const html = renderSingleMessage(hostileMsg, hostileChar);
    assert.ok(
      html.includes('data-avatar-fallback') || !html.includes('<img'),
      `expected delegated fallback attrs: ${html}`
    );
  });
});
