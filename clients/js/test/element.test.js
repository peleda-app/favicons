import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { Window } from "happy-dom";

let window;

before(() => {
  window = new Window({ url: "https://app.example.com/" });
  globalThis.window = window;
  globalThis.document = window.document;
  globalThis.HTMLElement = window.HTMLElement;
  globalThis.customElements = window.customElements;
});

after(async () => {
  for (const key of ["window", "document", "HTMLElement", "customElements"]) delete globalThis[key];
  await window.happyDOM.close();
});

test("the web component renders into its shadow root and follows its attributes", async () => {
  const { defineFaviconElement } = await import("../element.js");
  const Element = defineFaviconElement("favicon-icon", { baseUrl: "https://icons.example.com" });
  assert.ok(Element);
  assert.equal(defineFaviconElement("favicon-icon"), Element);

  const el = document.createElement("favicon-icon");
  el.setAttribute("domain", "github.com");
  el.setAttribute("shape", "superellipse");
  el.setAttribute("size", "40");
  document.body.append(el);
  const img = el.shadowRoot.querySelector("img");
  assert.equal(img.getAttribute("src"), "https://icons.example.com/color/github.com/superellipse");
  assert.equal(img.getAttribute("width"), "40");
  assert.equal(img.getAttribute("part"), "icon");

  el.removeAttribute("shape");
  el.setAttribute("mono", "");
  el.setAttribute("base-url", "https://other.example.com");
  const span = el.shadowRoot.querySelector("span");
  assert.match(span.getAttribute("style"), /mask:url\("https:\/\/other\.example\.com\/mono\/github\.com"\)/);
  assert.equal(span.getAttribute("aria-hidden"), "true");

  el.setAttribute("domain", "");
  assert.equal(el.shadowRoot.childNodes.length, 0);
});
