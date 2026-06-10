window.__codedocs_render_math = function () {
  if (typeof katex === "undefined") return;

  document.querySelectorAll(".math-inline").forEach(function (el) {
    var tex = el.textContent;
    try {
      katex.render(tex, el, { throwOnError: false, displayMode: false });
    } catch (e) {
      el.textContent = tex;
    }
  });

  document.querySelectorAll(".math-display").forEach(function (el) {
    var tex = el.textContent;
    try {
      katex.render(tex, el, { throwOnError: false, displayMode: true });
    } catch (e) {
      el.textContent = tex;
    }
  });
};

window.__codedocs_render_mermaid = function () {
  if (typeof mermaid === "undefined") return;

  document.querySelectorAll(".mermaid-block").forEach(function (el) {
    var code = el.getAttribute("data-mermaid");
    if (!code) return;
    var id = "mermaid-" + Math.random().toString(36).substr(2, 9);
    try {
      mermaid.render(id, code).then(function (svg) {
        el.innerHTML = svg.svg;
        el.removeAttribute("data-mermaid");
      }).catch(function () {
        el.innerHTML = "<pre>" + code + "</pre>";
      });
    } catch (e) {
      el.innerHTML = "<pre>" + code + "</pre>";
    }
  });
};

window.__codedocs_render_enhancements = function () {
  window.__codedocs_render_math();
  window.__codedocs_render_mermaid();
};
