(function () {
  "use strict";

  var changeEl = document.getElementById("change");
  var aiEl = document.getElementById("ai");
  var stateEl = document.getElementById("state");
  var requirementsEl = document.getElementById("requirements");
  var provenEl = document.getElementById("proven");
  var debtEl = document.getElementById("debt");
  var suppressedEl = document.getElementById("suppressed");
  var axesEl = document.getElementById("axes");
  var needsEl = document.getElementById("needs-human");
  var detailEl = document.getElementById("detail");
  var reviewerEl = document.getElementById("reviewer");
  var roleEl = document.getElementById("role");
  var digestEl = document.getElementById("artifact-digest");
  var statusEl = document.getElementById("status");

  function setText(el, value) {
    el.textContent = value == null ? "" : String(value);
  }

  function setStatus(message, kind) {
    setText(statusEl, message);
    if (kind) {
      statusEl.setAttribute("data-kind", kind);
    } else {
      statusEl.removeAttribute("data-kind");
    }
  }

  function getJson(path) {
    return fetch(path).then(function (response) {
      return response.json().then(function (body) {
        if (!response.ok) {
          throw new Error(body.error || response.statusText);
        }
        return body;
      });
    });
  }

  function postJson(path, payload) {
    return fetch(path, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    }).then(function (response) {
      return response.json().then(function (body) {
        if (!response.ok) {
          throw new Error(body.error || response.statusText);
        }
        return body;
      });
    });
  }

  function formatAi(ai) {
    if (!ai) {
      return "AI: unmeasured";
    }
    return (
      "AI: " +
      ai.planning_tokens +
      " planning · " +
      ai.runtime_tokens +
      " runtime · " +
      ai.browser_escape_calls +
      " browser · " +
      ai.vision_calls +
      " vision"
    );
  }

  function formatDebt(debt) {
    if (!debt) {
      return "unmeasured";
    }
    return (
      debt.new +
      " new · " +
      debt.returned +
      " returned · " +
      debt.fixed +
      " fixed"
    );
  }

  function formatAxes(axes) {
    if (!Array.isArray(axes) || axes.length === 0) {
      return "";
    }
    return axes
      .map(function (axis) {
        return axis.axis + ": " + axis.state;
      })
      .join(" · ");
  }

  function clear(el) {
    while (el.firstChild) {
      el.removeChild(el.firstChild);
    }
  }

  function decide(subject, decision) {
    var reviewer = reviewerEl.value.trim();
    var digest = digestEl.value.trim();
    if (!reviewer) {
      setStatus("reviewer is required", "error");
      return;
    }
    if (!digest) {
      setStatus("artifact digest is required", "error");
      return;
    }
    postJson("/api/v1/human-decisions", {
      id: "hd-" + Date.now(),
      reviewer: reviewer,
      role: roleEl.value,
      subject: subject,
      artifact_digest: digest,
      decision: decision,
      decided_at: new Date().toISOString(),
    })
      .then(function (body) {
        setStatus(
          "recorded " +
            body.decision +
            " on " +
            body.subject +
            (body.seal_eligible ? " (seal-eligible)" : "") +
            (body.escalates ? " (escalates)" : ""),
          ""
        );
      })
      .catch(function (err) {
        setStatus(err.message, "error");
      });
  }

  function showRequirement(requirement) {
    getJson("/api/v1/requirements/" + encodeURIComponent(requirement) + "/proofs")
      .then(function (body) {
        clear(detailEl);
        var heading = document.createElement("p");
        heading.textContent = body.requirement + " · " + body.change;
        detailEl.appendChild(heading);
        var list = document.createElement("ul");
        (body.proofs || []).forEach(function (proof) {
          var item = document.createElement("li");
          item.textContent =
            proof.obligation + " · " + proof.verdict + " · " + proof.id;
          list.appendChild(item);
        });
        detailEl.appendChild(list);
      })
      .catch(function (err) {
        setText(detailEl, err.message);
      });
  }

  function renderNeeds(items) {
    clear(needsEl);
    if (!Array.isArray(items) || items.length === 0) {
      var empty = document.createElement("li");
      empty.className = "muted";
      empty.textContent = "No exceptions for this change.";
      needsEl.appendChild(empty);
      return;
    }
    items.forEach(function (proof) {
      var item = document.createElement("li");
      var title = document.createElement("p");
      title.textContent =
        proof.requirement + ": " + proof.obligation + " · " + proof.verdict;
      item.appendChild(title);

      var facts = document.createElement("dl");
      [
        ["intent", proof.intent],
        ["surface", proof.surface],
        ["protection", proof.protection],
        ["proof", proof.proof],
        ["runtime", proof.runtime],
        ["coverage", proof.coverage],
        ["ui", proof.ui],
        ["a11y", proof.a11y],
        ["mutation", proof.mutation],
        ["code impact", proof.code_impact],
        ["behavior delta", proof.behavior_delta],
        ["visual region", proof.visual_region],
        ["failure reel", proof.failure_reel || "unmeasured"],
        ["cheapest next", proof.cheapest_next || "unmeasured"],
        [
          "source candidates",
          Array.isArray(proof.source_candidates) && proof.source_candidates.length
            ? proof.source_candidates.join(", ")
            : "unmeasured",
        ],
      ].forEach(function (pair) {
        var dt = document.createElement("dt");
        dt.textContent = pair[0];
        var dd = document.createElement("dd");
        dd.textContent = pair[1] == null ? "unmeasured" : String(pair[1]);
        facts.appendChild(dt);
        facts.appendChild(dd);
      });
      item.appendChild(facts);

      var inspect = document.createElement("button");
      inspect.type = "button";
      inspect.textContent = "Inspect requirement";
      inspect.addEventListener("click", function () {
        showRequirement(proof.requirement);
      });
      item.appendChild(inspect);

      [
        { label: "Expected change", decision: "accept_as_intended" },
        { label: "Bug", decision: "reject" },
        { label: "Update spec", decision: "request_product_decision" },
      ].forEach(function (action) {
        var button = document.createElement("button");
        button.type = "button";
        button.textContent = action.label;
        button.addEventListener("click", function () {
          decide(proof.obligation, action.decision);
        });
        item.appendChild(button);
      });
      needsEl.appendChild(item);
    });
  }

  function loadSummary(change) {
    getJson("/api/v1/changes/" + encodeURIComponent(change) + "/summary")
      .then(function (summary) {
        setText(
          stateEl,
          summary.state + " · " + summary.verdict + (summary.blocking ? " · blocking" : "")
        );
        setText(aiEl, formatAi(summary.ai));
        setText(requirementsEl, summary.requirements);
        setText(provenEl, summary.proven);
        setText(debtEl, formatDebt(summary.debt));
        setText(suppressedEl, summary.suppressed_passing);
        setText(axesEl, formatAxes(summary.axes));
        renderNeeds(summary.needs_attention);
      })
      .catch(function (err) {
        setStatus(err.message, "error");
      });
  }

  function loadChanges() {
    getJson("/api/v1/changes")
      .then(function (body) {
        clear(changeEl);
        var changes = body.changes || [];
        if (changes.length === 0) {
          setStatus("no OpenSpec changes", "error");
          return;
        }
        changes.forEach(function (id) {
          var option = document.createElement("option");
          option.value = id;
          option.textContent = id;
          changeEl.appendChild(option);
        });
        loadSummary(changeEl.value);
      })
      .catch(function (err) {
        setStatus(err.message, "error");
      });
  }

  changeEl.addEventListener("change", function () {
    loadSummary(changeEl.value);
  });

  loadChanges();
})();
