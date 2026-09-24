// Keyboard contract: every mathematical template has one explicit
// YaClue meaning. The future TeX adapter can consume this same table.
export const YACLUE_KEY_SEMANTICS = Object.freeze({
  add: { latex: '+', label: '+', yaclue: 'left+right' },
  subtract: { latex: '-', label: '−', yaclue: 'left-right' },
  multiply: { keycap: '\\times', latex: '\\yaclueMultiply{#@}{#0}', yaclue: 'left*right' },
  equation: { keycap: '=', latex: '\\yaclueEquation{#@}{#0}', label: '=', yaclue: 'left==right' },
  decimal: { latex: '.', label: '.', yaclue: 'decimal point' },
  fraction: { keycap: '\\frac{#0}{#0}', latex: '\\frac{#@}{#0}', class: 'small', yaclue: 'numerator/denominator' },
  power: { keycap: 'x^{#0}', latex: '#@^{#0}', class: 'small', yaclue: 'base^exponent' },
  square: { keycap: 'x^2', latex: '#@^2', yaclue: 'base^2' },
  root: { keycap: '\\sqrt{#0}', latex: '\\sqrt{#0}', class: 'small', yaclue: 'Sqrt(expr)' },
  absolute: { keycap: '\\lvert#0\\rvert', latex: '\\yaclueAbs{#0}', class: 'small', yaclue: 'Abs(expr)' },
  norm: { keycap: '\\lVert#0\\rVert', latex: '\\yaclueNorm{#0}', class: 'small', yaclue: 'Norm(vector)' },
  sin: { keycap: '\\sin', latex: '\\sin\\left(#0\\right)', yaclue: 'Sin(expr)' },
  cos: { keycap: '\\cos', latex: '\\cos\\left(#0\\right)', yaclue: 'Cos(expr)' },
  tan: { keycap: '\\tan', latex: '\\tan\\left(#0\\right)', yaclue: 'Tan(expr)' },
  ln: { keycap: '\\ln', latex: '\\ln\\left(#0\\right)', yaclue: 'Ln(expr)' },
  exp: { keycap: '\\exp', latex: '\\exp\\left(#0\\right)', yaclue: 'Exp(expr)' },
  arcsin: { keycap: '\\sin^{-1}', class: 'small', latex: '\\yaclueArcSin{#0}', yaclue: 'ArcSin(expr)' },
  arccos: { keycap: '\\cos^{-1}', class: 'small', latex: '\\yaclueArcCos{#0}', yaclue: 'ArcCos(expr)' },
  arctan: { keycap: '\\tan^{-1}', class: 'small', latex: '\\yaclueArcTan{#0}', yaclue: 'ArcTan(expr)' },
  floor: { keycap: '\\lfloor x\\rfloor', latex: '\\yaclueFloor{#0}', yaclue: 'Floor(expr)' },
  ceil: { keycap: '\\lceil x\\rceil', latex: '\\yaclueCeil{#0}', yaclue: 'Ceil(expr)' },
  gamma: { keycap: '\\Gamma', latex: '\\Gamma\\left(#0\\right)', yaclue: 'Gamma(expr)' },
  zeta: { keycap: '\\zeta', latex: '\\zeta\\left(#0\\right)', yaclue: 'Zeta(expr)' },
  lambertW: { keycap: 'W', latex: '\\yaclueLambertW{#0}', yaclue: 'LambertW(expr)' },
  derivative: { keycap: '\\frac{\\mathrm{d}}{\\mathrm{d}x}', class: 'small', latex: '\\frac{\\mathrm{d}}{\\mathrm{d}x}', yaclue: 'D(x)expr' },
  differential: { keycap: '\\mathrm{d}', latex: '\\mathrm{d}', yaclue: 'integral differential; follow with a variable' },
  nthDerivative: { keycap: '\\frac{\\mathrm{d}^{2}}{\\mathrm{d}x^{2}}', class: 'small', latex: '\\frac{\\mathrm{d}^{2}}{\\mathrm{d}x^{2}}', yaclue: 'D(x,2)expr' },
  integral: { keycap: '\\int', class: 'integral-key', latex: '\\int', yaclue: 'Integrate(x)expr dx' },
  definiteIntegral: { keycap: '\\int_{a}^{b}', class: 'integral-key', latex: '\\int_{#0}^{#0}', yaclue: 'Integrate(x,a,b)expr dx' },
  limit: { keycap: '\\lim', latex: '\\lim_{x\\to #0}', yaclue: 'Limit(x,a)expr' },
  sum: { keycap: '\\sum', latex: '\\sum_{k=#0}^{#0}', yaclue: 'Sum(k,a,b,expr)' },
  comma: { latex: ',', label: ',', yaclue: 'list/argument separator' },
  assumptionSeparator: { latex: ';', label: ';', yaclue: 'one-shot assumption separator', assumptionSample: 'x;x>0' },
  assumptionPositive: { latex: '>', label: '>', yaclue: 'positive assumption', assumptionSample: 'x;x>0' },
  assumptionNegative: { latex: '<', label: '<', yaclue: 'negative assumption', assumptionSample: 'x;x<0' },
  assumptionNonZero: { keycap: '\\ne', latex: '\\ne', yaclue: 'non-zero assumption', assumptionSample: 'x;x\\ne0' },
  list: { keycap: '[#0]', label: '<small>[&thinsp;&hellip;&thinsp;]</small>', latex: '\\yaclueList{#0}', yaclue: '{item,...}' },
  vector: { keycap: 'v', label: 'Vec', latex: '\\yaclueVector{\\begin{bmatrix}#0\\\\#0\\end{bmatrix}}', yaclue: '{component,...}' },
  matrix: { keycap: 'M', label: 'Mat', latex: '\\yaclueMatrix{\\begin{bmatrix}#0\\end{bmatrix}}', yaclue: '{{cell,...},...}' },
  sign: { label: 'sgn', latex: '\\operatorname{sgn}\\left(#0\\right)', yaclue: 'Sign(expr)' },
  round: { label: 'Round', latex: '\\operatorname{round}\\left(#0\\right)', yaclue: 'Round(expr)' },
  min: { label: 'Min', latex: '\\min\\left(#0,#0\\right)', yaclue: 'Min(a,b)' },
  max: { label: 'Max', latex: '\\max\\left(#0,#0\\right)', yaclue: 'Max(a,b)' },
  div: { label: 'Div', latex: '\\operatorname{Div}\\left(#0,#0\\right)', yaclue: 'Div(a,b)' },
  mod: { label: 'Mod', latex: '\\operatorname{Mod}\\left(#0,#0\\right)', yaclue: 'Mod(a,b)' },
  gcd: { label: 'GCD', latex: '\\operatorname{Gcd}\\left(#0,#0\\right)', yaclue: 'Gcd(a,b)' },
  lcm: { label: 'LCM', latex: '\\operatorname{Lcm}\\left(#0,#0\\right)', yaclue: 'Lcm(a,b)' },
  numer: { label: 'Numer', latex: '\\operatorname{Numer}\\left(#0\\right)', yaclue: 'Numer(expr)' },
  denom: { label: 'Denom', latex: '\\operatorname{Denom}\\left(#0\\right)', yaclue: 'Denom(expr)' },
  bernoulli: { keycap: 'B_n', latex: '\\operatorname{Bernoulli}\\left(#0\\right)', yaclue: 'Bernoulli(n)' },
  euler: { keycap: 'E_n', latex: '\\operatorname{Euler}\\left(#0\\right)', yaclue: 'Euler(n)' },
  leftLimit: { keycap: '\\lim^-', latex: '\\yaclueLeftLimit{x}{#0}{#@}', yaclue: 'Limit(x,a,Left)expr' },
  rightLimit: { keycap: '\\lim^+', latex: '\\yaclueRightLimit{x}{#0}{#@}', yaclue: 'Limit(x,a,Right)expr' },
  taylor: { label: 'Taylor', latex: '\\yaclueTaylor{x}{#0}{#0}{#@}', yaclue: 'Taylor(x,a,n)expr' },
  substitute: { label: 'Subst', latex: '\\yaclueSubstitute{x}{#0}{#@}', yaclue: 'Subst(x,replacement)expr' },
  doubleIntegral: { keycap: '\\iint', class: 'integral-key', latex: '\\yaclueDoubleIntegral{x}{#0}{#0}{y}{#0}{#0}{#@}', yaclue: 'DoubleIntegral(expr,x,a,b,y,c,d)' },
  polarIntegral: { keycap: '\\iint_{\\mathrm{pol}}', class: 'integral-key', latex: '\\yacluePolarIntegral{x}{y}{r}{\\theta}{#0}{#0}{#0}{#0}{#@}', yaclue: 'PolarIntegral(expr,x,y,r,theta,r0,r1,t0,t1)' },
  infinity: { keycap: '\\infty', latex: '\\infty', yaclue: 'Infinity' },
  principalValue: { keycap: '\\operatorname{PV}\\!\\int', class: 'integral-key', latex: '\\yacluePrincipalValueIntegral{x}{#0}{#0}{#@}', yaclue: 'PrincipalValueIntegral(expr,x,a,b)' },
  partial: { keycap: '\\frac{\\partial}{\\partial x}', class: 'small', latex: '\\frac{\\partial}{\\partial x}', yaclue: 'D(x)expr' },
  gradient: { label: 'Grad', latex: '\\yaclueGradient{#0}{#0}', yaclue: 'Gradient(expr,vars)' },
  jacobian: { label: 'Jac', latex: '\\yaclueJacobian{#0}{#0}', yaclue: 'Jacobian(expr,vars)' },
  hessian: { label: 'Hess', latex: '\\yaclueHessian{#0}{#0}', yaclue: 'Hessian(expr,vars)' },
  divergence: { label: 'Div∇', latex: '\\yaclueDivergence{#0}{#0}', yaclue: 'Divergence(expr,vars)' },
  curl: { label: 'Curl', latex: '\\yaclueCurl{#0}{#0}', yaclue: 'Curl(expr,vars)' },
  directional: { keycap: 'D_u', latex: '\\yaclueDirectionalDerivative{#0}{#0}{#0}', yaclue: 'DirectionalDerivative(expr,vars,dir)' },
  scalarLine: { label: 'SLine', latex: '\\yaclueScalarLineIntegral{#0}{#0}{#0}{t}{#0}{#0}', yaclue: 'ScalarLineIntegral(expr,vars,path,t,a,b)' },
  vectorLine: { label: 'VLine', latex: '\\yaclueVectorLineIntegral{#0}{#0}{#0}{t}{#0}{#0}', yaclue: 'VectorLineIntegral(field,vars,path,t,a,b)' },
  scalarSurface: { label: 'SSurf', latex: '\\yaclueScalarSurfaceIntegral{#0}{#0}{#0}{#0}{#0}{#0}', yaclue: 'ScalarSurfaceIntegral(expr,vars,surface,params,lower,upper)' },
  vectorSurface: { label: 'VSurf', latex: '\\yaclueVectorSurfaceIntegral{#0}{#0}{#0}{#0}{#0}{#0}', yaclue: 'VectorSurfaceIntegral(field,vars,surface,params,lower,upper[,orientation])' },
  dot: { label: 'Dot', latex: '\\yaclueDot{#0}{#0}', yaclue: 'Dot(u,v)' },
  cross: { label: 'Cross', latex: '\\yaclueCross{#0}{#0}', yaclue: 'CrossProduct(u,v)' },
  outer: { label: 'Outer', latex: '\\yaclueOuter{#0}{#0}', yaclue: 'Outer(u,v)' },
  normalize: { label: 'Unit', latex: '\\operatorname{Normalize}\\left(#0\\right)', yaclue: 'Normalize(v)' },
  pnorm: { label: 'p-Norm', latex: '\\yacluePNorm{#0}{#0}', yaclue: 'PNorm(v,p)' },
  transpose: { label: 'Trans', latex: '\\yaclueTranspose{#0}', yaclue: 'Transpose(matrix)' },
  determinant: { label: 'Det', latex: '\\yaclueDeterminant{#0}', yaclue: 'Determinant(matrix)' },
  inverse: { label: 'Inv', latex: '\\yaclueInverse{#0}', yaclue: 'Inverse(matrix)' },
  rank: { label: 'Rank', latex: '\\operatorname{rank}\\left(#0\\right)', yaclue: 'Rank(matrix)' },
  rref: { label: 'RREF', latex: '\\operatorname{RREF}\\left(#0\\right)', yaclue: 'RREF(matrix)' },
  trace: { label: 'Trace', latex: '\\operatorname{tr}\\left(#0\\right)', yaclue: 'Trace(matrix)' },
  eigen: { label: 'Eigen', latex: '\\operatorname{EigenValues}\\left(#0\\right)', yaclue: 'EigenValues(matrix)' },
  nullSpace: { label: 'Null', latex: '\\operatorname{NullSpace}\\left(#0\\right)', yaclue: 'NullSpace(matrix)' },
  matrixSolve: { label: 'MSolve', latex: '\\operatorname{MatrixSolve}\\left(#0,#0\\right)', yaclue: 'MatrixSolve(matrix,vector)' },
  rowReduce: { label: 'RowRed', latex: '\\operatorname{RowReduce}\\left(#0\\right)', yaclue: 'RowReduce(matrix)' },
  columnSpace: { label: 'ColSp', latex: '\\operatorname{ColumnSpace}\\left(#0\\right)', yaclue: 'ColumnSpace(matrix)' },
  eigenSpaces: { label: 'EigSp', latex: '\\operatorname{EigenSpaces}\\left(#0\\right)', yaclue: 'EigenSpaces(matrix)' },
  pldu: { label: 'PLDU', latex: '\\operatorname{PLDU}\\left(#0\\right)', yaclue: 'PLDU(matrix)' },
  cholesky: { label: 'Chol', latex: '\\operatorname{Cholesky}\\left(#0\\right)', yaclue: 'Cholesky(matrix)' },
  gramSchmidt: { label: 'Gram', latex: '\\operatorname{GramSchmidt}\\left(#0\\right)', yaclue: 'GramSchmidt(matrix)' },
  orthogonal: { label: 'Orth', latex: '\\operatorname{OrthogonalBasis}\\left(#0\\right)', yaclue: 'OrthogonalBasis(matrix)' },
  orthonormal: { label: 'OrthoN', latex: '\\operatorname{OrthonormalBasis}\\left(#0\\right)', yaclue: 'OrthonormalBasis(matrix)' },
  factors: { label: 'Factors', latex: '\\operatorname{Factors}\\left(#0\\right)', yaclue: 'Factors(matrix)' },
  matrixPower: { label: 'MatPow', latex: '\\operatorname{MatrixPower}\\left(#0,#0\\right)', yaclue: 'MatrixPower(matrix,n)' },
  diagonal: { label: 'Diag', latex: '\\operatorname{Diagonal}\\left(#0\\right)', yaclue: 'Diagonal(matrix)' },
  identity: { label: 'IdMat', latex: '\\operatorname{Identity}\\left(#0\\right)', yaclue: 'Identity(n)' },
  factor: { label: 'Fact', latex: '\\operatorname{Factor}\\left(#0\\right)', yaclue: 'Factor(expr)' },
  expand: { label: 'Expand', latex: '\\operatorname{Expand}\\left(#0\\right)', yaclue: 'Expand(expr)' },
  simplify: { label: 'Simp', latex: '\\operatorname{Simplify}\\left(#0\\right)', yaclue: 'Simplify(expr)' },
  tidy: { label: 'Tidy', latex: '\\operatorname{Tidy}\\left(#0\\right)', yaclue: 'Tidy(expr)' },
  apart: { label: 'Apart', latex: '\\operatorname{Apart}\\left(#0,x\\right)', yaclue: 'Apart(expr,x)' },
  solve: { label: 'Solve', latex: '\\operatorname{Solve}\\left(#0,x\\right)', yaclue: 'Solve(equation,var)' },
  numeric: { label: 'N', latex: '\\operatorname{N}\\left(#0\\right)', yaclue: 'N(expr[,precision])' },
  findRoot: { label: 'Root', latex: '\\operatorname{FindRoot}\\left(#0,x,#0\\right)', yaclue: 'FindRoot(expr,var,initial)' },
  ode: { label: 'ODE', latex: '\\operatorname{OdeSolve}\\left(#0\\right)', yaclue: 'OdeSolve(equation)' },
  plot: { label: 'Plot', latex: '\\operatorname{Plot}\\left(#0,x,#0,#0\\right)', yaclue: 'Plot(expr,var,a,b)' },
  extrema: { label: 'Extr', latex: '\\operatorname{Extrema}\\left(#0,x,y\\right)', yaclue: 'Extrema(expr,var1,var2)' },
  lagrange: { label: 'Lagr', latex: '\\operatorname{Lagrange}\\left(#0,#0,x,y\\right)', yaclue: 'Lagrange(objective,constraint,var1,var2)' },
});
const semanticKey = (name, options = {}) => {
  const semantic = YACLUE_KEY_SEMANTICS[name];
  const textKey = /[A-Za-z]{2,}/.test(semantic.label ?? '');
  return {
    latex: semantic.keycap ?? semantic.latex,
    insert: semantic.latex,
    label: semantic.label,
    class: [semantic.class, textKey ? 'text-key' : ''].filter(Boolean).join(' '),
    width: semantic.width,
    tooltipKey: `keyboard.key.${name}`,
    ...options,
  };
};
// Every page shares the same four right-hand columns. An extra row makes the
// touch targets wider without dropping any of the twenty page-specific keys.
const commonRightRows = [
  ['[7]', '[8]', '[9]', { label: '[backspace]', width: 1 }],
  ['[4]', '[5]', '[6]', semanticKey('multiply')],
  ['[1]', '[2]', '[3]', semanticKey('add')],
  ['[left]', '[0]', '[right]', semanticKey('subtract')],
  [semanticKey('comma'), semanticKey('decimal'), semanticKey('equation'), { label: '[action]', width: 1 }],
];
const compactKey = (name) => typeof name === 'string' && YACLUE_KEY_SEMANTICS[name]
  ? semanticKey(name)
  : name;
const makeKeyboard = (id, label, keys) => ({
  id: 'yaclue-' + id,
  label,
  tooltipKey: `keyboard.${id}`,
  displayEditToolbar: false,
  rows: commonRightRows.map((right, row) => [
    ...keys.slice(row * 4, row * 4 + 4).map(compactKey),
    ...right,
  ]),
});
const yaclueKeyboard = makeKeyboard('core', 'Basic', [
  'fraction', 'root', 'power', 'square',
  'sin', 'cos', 'tan', 'absolute',
  'ln', 'exp', 'norm', '\\pi',
  '(', ')', { latex: 'x', tooltip: 'Variable x' }, { latex: 'y', tooltip: 'Variable y' },
  { latex: 'z', tooltip: 'Variable z' }, { latex: 't', tooltip: 'Variable t' },
  { latex: 'n', tooltip: 'Variable n' }, 'infinity',
]);
const yaclueCalculusKeyboard = makeKeyboard('calculus', 'Calculus', [
  'fraction', 'root', 'power', 'square',
  'derivative', 'nthDerivative', 'integral', 'definiteIntegral',
  'differential', 'limit', 'sum', 'absolute',
  '(', ')', { latex: 'x', tooltip: 'Variable x' }, { latex: 'y', tooltip: 'Variable y' },
  { latex: 'z', tooltip: 'Variable z' }, { latex: 't', tooltip: 'Variable t' },
  { latex: 'n', tooltip: 'Variable n' }, '\\pi',
]);
const yaclueFunctionKeyboard = makeKeyboard('functions', 'Func', [
  'fraction', 'root', 'power', 'square',
  'arcsin', 'arccos', 'arctan', 'gamma',
  'zeta', 'lambertW', 'floor', 'ceil',
  '(', ')', { latex: 'x', tooltip: 'Variable x' }, { latex: 'y', tooltip: 'Variable y' },
  { latex: 'z', tooltip: 'Variable z' }, { latex: 't', tooltip: 'Variable t' },
  { latex: 'n', tooltip: 'Variable n' }, '\\pi',
]);
const yaclueStructureKeyboard = makeKeyboard('structure', 'Struct', [
  'fraction', 'root', 'power', 'square',
  'matrix',
  { label: '<small>+Row</small>', class: 'text-key', command: 'performWithFeedback(addRowAfter)', tooltipKey: 'keyboard.hint.addRow' },
  {
    label: '<small>+Col</small>',
    class: 'text-key',
    command: ['typedText', '&?', { focus: true, feedback: true, simulateKeystroke: true }],
    tooltipKey: 'keyboard.hint.addCol',
  },
  'vector',
  'list', 'absolute', 'norm', '\\pi',
  '(', ')', { latex: 'x', tooltip: 'Variable x' }, { latex: 'y', tooltip: 'Variable y' },
  { latex: 'z', tooltip: 'Variable z' }, { latex: 't', tooltip: 'Variable t' },
  { latex: 'n', tooltip: 'Variable n' }, 'transpose',
]);

const yaclueFunctionPlusKeyboard = makeKeyboard('functions-plus', 'Func+', [
  'sign', 'round', 'min', 'max',
  'div', 'mod', 'gcd', 'lcm',
  'numer', 'denom', 'bernoulli', 'euler',
  'gamma', 'zeta', 'lambertW', { latex: 'x' },
  { latex: 'n' }, '(', ')', '\\pi',
]);
const yaclueCalculusPlusKeyboard = makeKeyboard('calculus-plus', 'Calc+', [
  'partial', 'leftLimit', 'rightLimit', 'taylor',
  'substitute', 'doubleIntegral', 'polarIntegral', 'infinity',
  'principalValue', 'differential', 'integral', 'definiteIntegral',
  'derivative', 'nthDerivative', 'limit', { latex: 'x' },
  { latex: 'y' }, { latex: 'r' }, { latex: '\\theta' }, '\\pi',
]);
const yaclueMultivariableKeyboard = makeKeyboard('multivariable', 'Multi', [
  'gradient', 'jacobian', 'hessian', 'divergence',
  'curl', 'directional', 'scalarLine', 'vectorLine',
  'scalarSurface', 'vectorSurface', 'vector', 'list',
  'matrix', 'norm', { latex: 'x' }, { latex: 'y' },
  { latex: 'z' }, { latex: 'u' }, { latex: 'v' }, '(',
]);
const yaclueLinearKeyboard = makeKeyboard('linear', 'Linear', [
  'dot', 'cross', 'outer', 'normalize',
  'pnorm', 'transpose', 'determinant', 'inverse',
  'rank', 'rref', 'trace', 'eigen',
  'nullSpace', 'columnSpace', 'matrixSolve', 'rowReduce',
  'pldu', 'cholesky', 'gramSchmidt', 'orthonormal',
]);
const yaclueActionsKeyboard = makeKeyboard('actions', 'Actions', [
  'solve', 'factor', 'expand', 'simplify',
  'tidy', 'apart', 'numeric', 'findRoot',
  'ode', 'plot', 'extrema', 'lagrange',
  'matrixPower', 'diagonal', 'identity', 'list',
  'matrix', '(', ')', '\\pi',
]);
const yaclueAlphabetKeyboard = makeKeyboard('alphabet', 'ABC', [
  ...'abcdefghijklmopqrsuw'.split('').map((latex) => ({ latex })),
]);
const yaclueAssumptionKeyboard = makeKeyboard('assumptions', 'Given', [
  'assumptionSeparator', 'assumptionPositive', 'assumptionNegative', 'assumptionNonZero',
  { latex: 'x' }, { latex: 'y' }, { latex: 'z' }, { latex: 'n' },
  { latex: 't' }, 'fraction', 'root', 'power',
  'square', 'absolute', '(', ')',
  'list', 'norm', '\\pi', 'infinity',
]);


export const yaclueKeyboardLayouts = [
  yaclueKeyboard,
  yaclueFunctionKeyboard,
  yaclueCalculusKeyboard,
  yaclueStructureKeyboard,
  yaclueAlphabetKeyboard,
  yaclueAssumptionKeyboard,
  yaclueFunctionPlusKeyboard,
  yaclueCalculusPlusKeyboard,
  yaclueLinearKeyboard,
  yaclueMultivariableKeyboard,
  yaclueActionsKeyboard,
];

const keyHint = (key, translate) => {
  if (typeof key === 'string') {
    if (/^\[\d\]$/.test(key)) return { label: key, tooltip: translate('keyboard.hint.digit', { symbol: key[1] }) };
    const shortcut = { '[left]': 'moveLeft', '[right]': 'moveRight' }[key];
    if (shortcut) return { label: key, tooltip: translate(`keyboard.hint.${shortcut}`) };
    const symbol = { '(': 'openParenthesis', ')': 'closeParenthesis', '\\pi': 'pi' }[key];
    return symbol ? { latex: key, tooltip: translate(`keyboard.hint.${symbol}`) } : key;
  }
  const { tooltipKey, tooltipArgs, ...options } = key;
  const keyName = tooltipKey
    || ({ '[backspace]': 'keyboard.hint.backspace', '[action]': 'keyboard.hint.calculate' }[key.label])
    || (/^[a-z]$/.test(key.latex || '') || key.latex === '\\theta' ? 'keyboard.hint.variable' : null);
  if (!keyName) return options;
  const symbol = key.latex === '\\theta' ? 'θ' : key.latex;
  return { ...options, tooltip: translate(keyName, tooltipArgs || { symbol }) };
};

export const localizeKeyboardLayouts = (translate) => yaclueKeyboardLayouts.map(({ tooltipKey, ...layout }) => ({
  ...layout,
  tooltip: translate(tooltipKey),
  rows: layout.rows.map((row) => row.map((key) => keyHint(key, translate))),
}));
