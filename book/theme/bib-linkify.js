// Turn bibliography entry titles into links to the DOI/URL and remove the
// trailing bare URL. Data source: window.__BIB_LINKS (generated from
// references.bib by book/theme/bib-links.js).
(function () {
  if (!window.__BIB_LINKS) return;
  var entries = document.querySelectorAll('.csl-entry');
  if (!entries.length) return;

  function normalize(s) {
    return s
      .replace(/[\u201C\u201D\u2018\u2019"'`]/g, '')
      .replace(/\s+/g, ' ')
      .trim()
      .toLowerCase();
  }

  entries.forEach(function (entry) {
    var key = entry.id;
    var info = window.__BIB_LINKS[key];
    if (!info) return;

    var html = entry.innerHTML;

    // 1. Strip trailing bare URL (optionally followed by period/whitespace).
    var urlEscaped = info.url.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    var urlRe = new RegExp('\\s*' + urlEscaped + '\\.?\\s*$');
    html = html.replace(urlRe, '');

    // Also strip a trailing bare-URL form even if it differs from info.url
    // (e.g., CSL dropped the trailing period or reformatted). Matches
    // "https://... " at the very end.
    html = html.replace(/\s+https?:\/\/\S+\.?\s*$/, '');

    // 2. Wrap the title in an anchor. Try exact match first, then a
    // normalized, quote-insensitive search.
    var title = info.title;
    var linked = false;

    var idx = html.indexOf(title);
    if (idx !== -1) {
      html = html.slice(0, idx) +
        '<a href="' + info.url + '">' + html.slice(idx, idx + title.length) + '</a>' +
        html.slice(idx + title.length);
      linked = true;
    } else {
      // Search by normalized form. Walk the string trying substrings of the
      // same length as the title and compare normalized. Coarse but works.
      var target = normalize(title);
      var n = html.length;
      var tlen = title.length;
      // Allow a little slack for quote-characters introduced by the renderer.
      for (var span = tlen; span <= tlen + 4 && !linked; span++) {
        for (var i = 0; i + span <= n; i++) {
          var slice = html.slice(i, i + span);
          if (slice.indexOf('<') !== -1) continue;
          if (normalize(slice) === target) {
            html = html.slice(0, i) +
              '<a href="' + info.url + '">' + slice + '</a>' +
              html.slice(i + span);
            linked = true;
            break;
          }
        }
      }
    }

    if (!linked) {
      // Fall back to appending the URL as a link so readers retain access.
      html = html.replace(/\s*$/, '') +
        ' <a href="' + info.url + '">' + info.url + '</a>';
    }

    entry.innerHTML = html;
  });
})();
