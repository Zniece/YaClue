const { invoke } = window.__TAURI__.core;

const inputEl = document.querySelector("#expr");
const stepsListEl = document.querySelector("#steps-list");
const errorEl = document.querySelector("#error");

// 规则名 → 中文标签(求导/积分共用)
const RULE_LABELS = {
  "sum-rule": "和法则",
  "product-rule": "乘积法则",
  "constant-multiple-rule": "常数倍法则",
  "quotient-rule": "商法则",
  "power-rule": "幂法则",
  "exponential-rule": "指数法则",
  "sin-rule": "正弦",
  "cos-rule": "余弦",
  "exp-rule": "指数函数",
  "ln-rule": "对数",
  "sqrt-rule": "根式",
  "identity-rule": "D(x) = 1",
  "const-rule": "常数 = 0",
  "const-integral-rule": "常数积分",
  "arctan-rule": "反正切积分",
  "u-sub-rule": "换元积分",
  "back-sub-rule": "回代",
  "parts-rule": "分部积分",
  direct: "直接计算",
  simplify: "化简",
};

async function run() {
  const expr = inputEl.value.trim();
  if (!expr) return;
  errorEl.textContent = "";
  stepsListEl.innerHTML = "";
  try {
    const cmd = document.querySelector("#mode").value === "i" ? "derive_integrals" : "derive_steps";
    const steps = await invoke(cmd, { expr });
    renderSteps(steps);
  } catch (e) {
    errorEl.textContent = typeof e === "string" ? e : JSON.stringify(e);
  }
}

function renderSteps(steps) {
  steps.forEach((s, i) => {
    const div = document.createElement("div");
    div.className = "step";

    const label = document.createElement("span");
    label.className = "rule";
    // 声明式文案优先;未登记键回退为规则标签/规则键
    label.textContent = `${i + 1}. ${s.why || RULE_LABELS[s.rule] || s.rule}`;

    const math = document.createElement("span");
    math.className = "math";

    div.appendChild(label);
    div.appendChild(math);
    stepsListEl.appendChild(div);

    katex.render(s.tex, math, { throwOnError: false, displayMode: true });
  });
}

document.querySelector("#go").addEventListener("click", run);
inputEl.addEventListener("keydown", (e) => {
  if (e.key === "Enter") run();
});
