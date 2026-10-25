// Browser API shims needed by UtaFormatix3's Kotlin/JS build when it runs
// under Node instead of a browser.  Only the surface the library actually
// touches is implemented: FileReader (text / binary string / ArrayBuffer).
class NodeFileReader {
  constructor() {
    this.result = null;
    this.error = null;
    this.onload = null;
    this.onloadend = null;
    this.onerror = null;
    this.onloadstart = null;
    this.onprogress = null;
    this.readyState = 0;
  }

  _finish() {
    this.readyState = 2;
    this.onloadend?.({ target: this });
  }

  _fail(error) {
    this.readyState = 2;
    this.error = error;
    this.onerror?.({ target: this, error });
    this.onloadend?.({ target: this });
  }

  readAsText(file, encoding) {
    this.readyState = 1;
    file.arrayBuffer().then((buffer) => {
      const bytes = new Uint8Array(buffer);
      let text;
      try {
        text = new TextDecoder(encoding || "utf-8").decode(bytes);
      } catch {
        text = new TextDecoder("utf-8").decode(bytes);
      }
      this.result = text;
      this.onload?.({ target: this });
      this._finish();
    }).catch((error) => this._fail(error));
  }

  readAsBinaryString(file) {
    this.readyState = 1;
    file.arrayBuffer().then((buffer) => {
      const bytes = new Uint8Array(buffer);
      let text = "";
      const chunk = 0x8000;
      for (let index = 0; index < bytes.length; index += chunk) {
        text += String.fromCharCode(...bytes.subarray(index, index + chunk));
      }
      this.result = text;
      this.onload?.({ target: this });
      this._finish();
    }).catch((error) => this._fail(error));
  }

  readAsArrayBuffer(file) {
    this.readyState = 1;
    file.arrayBuffer().then((buffer) => {
      this.result = buffer;
      this.onload?.({ target: this });
      this._finish();
    }).catch((error) => this._fail(error));
  }

  abort() {
    this.readyState = 2;
    this._finish();
  }
}

if (typeof globalThis.FileReader === "undefined") {
  globalThis.FileReader = NodeFileReader;
}

// VSQX / CCS / S5P / MusicXML / TSSLN generation builds its output through the
// DOM (it parses an XML template and fills it in), so Node needs an XML DOM.
const xmldom = await import("@xmldom/xmldom");
if (typeof globalThis.DOMParser === "undefined") {
  globalThis.DOMParser = xmldom.DOMParser;
  globalThis.XMLSerializer = xmldom.XMLSerializer;
  // The Kotlin code checks `document instanceof XMLDocument`; the class that
  // @xmldom/xmldom's parser instantiates is exactly what that check expects.
  const probe = new xmldom.DOMImplementation().createDocument(null, null, null);
  globalThis.XMLDocument = probe.constructor;
  // The generators type-check DOM nodes (`tmp instanceof Element`, ...), so the
  // classes the parser actually instantiates have to be global as well.
  const element = probe.createElement("probe");
  const textNode = probe.createTextNode("probe");
  globalThis.Element = element.constructor;
  globalThis.Node = Object.getPrototypeOf(Object.getPrototypeOf(element)).constructor;
  globalThis.Text = textNode.constructor;
  if (typeof globalThis.Element.prototype.insertAdjacentElement !== "function") {
    globalThis.Element.prototype.insertAdjacentElement = function insertAdjacentElement(position, node) {
      switch (position) {
        case "beforebegin":
          this.parentNode.insertBefore(node, this);
          break;
        case "afterbegin":
          this.insertBefore(node, this.firstChild);
          break;
        case "beforeend":
          this.appendChild(node);
          break;
        case "afterend":
          this.parentNode.insertBefore(node, this.nextSibling);
          break;
        default:
          throw new Error(`insertAdjacentElement: bad position ${position}`);
      }
      return node;
    };
  }
  if (typeof globalThis.document === "undefined") {
    globalThis.document = probe;
  }
}
