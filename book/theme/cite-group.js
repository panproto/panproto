(function() {
  function groupCitations() {
    var cites = document.querySelectorAll('a.cite');
    if (!cites.length) return;

    var groups = [];
    var cur = [cites[0]];

    for (var i = 1; i < cites.length; i++) {
      var prev = cur[cur.length - 1];
      var next = cites[i];
      var between = '';
      var node = prev.nextSibling;
      var adjacent = false;
      while (node && node !== next) {
        if (node.nodeType === 3) {
          between += node.textContent;
          node = node.nextSibling;
        } else {
          break;
        }
      }
      if (node === next && between.trim() === '') {
        cur.push(next);
      } else {
        groups.push(cur);
        cur = [next];
      }
    }
    groups.push(cur);

    groups.forEach(function(group) {
      var span = document.createElement('span');
      span.className = 'cite-group';
      group[0].parentNode.insertBefore(span, group[0]);

      /* remove whitespace text nodes between adjacent cites */
      group.forEach(function(cite) {
        var prev = cite.previousSibling;
        while (prev && prev !== span && prev.nodeType === 3 && prev.textContent.trim() === '') {
          var rm = prev;
          prev = prev.previousSibling;
          rm.remove();
        }
      });

      span.appendChild(document.createTextNode('['));
      group.forEach(function(cite, i) {
        span.appendChild(cite);
        if (i < group.length - 1) {
          span.appendChild(document.createTextNode(', '));
        }
      });
      span.appendChild(document.createTextNode(']'));
    });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', groupCitations);
  } else {
    groupCitations();
  }
})();
