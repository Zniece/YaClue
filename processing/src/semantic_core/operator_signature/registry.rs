use super::super::object_state::Requirement;
use super::{
    ApplicationForm, BinderDescriptor, CapabilityId, ObjectNativeRoute, OperatorDescriptor,
    OperatorId, OperatorSlotSignature, ScopeArgument, ValueArgument,
};

const CALL: &[ApplicationForm] = &[ApplicationForm::Call];
const BODIED: &[ApplicationForm] = &[ApplicationForm::Bodied];
const BODIED_AND_CONVENTIONAL: &[ApplicationForm] = &[
    ApplicationForm::Bodied,
    ApplicationForm::ConventionalValueFirst,
];
const NO_BINDERS: &[BinderDescriptor] = &[];
const FIRST_ARGUMENT_BINDS_LAST: &[BinderDescriptor] = &[BinderDescriptor {
    binder_argument: 0,
    scope_argument: ScopeArgument::Last,
}];
const SECOND_ARGUMENT_BINDS_FIRST: &[BinderDescriptor] = &[BinderDescriptor {
    binder_argument: 1,
    scope_argument: ScopeArgument::First,
}];
const SECOND_AND_THIRD_BIND_FIRST: &[BinderDescriptor] = &[
    BinderDescriptor {
        binder_argument: 1,
        scope_argument: ScopeArgument::First,
    },
    BinderDescriptor {
        binder_argument: 2,
        scope_argument: ScopeArgument::First,
    },
];
const DOUBLE_INTEGRAL_BINDERS: &[BinderDescriptor] = &[
    BinderDescriptor {
        binder_argument: 1,
        scope_argument: ScopeArgument::First,
    },
    BinderDescriptor {
        binder_argument: 4,
        scope_argument: ScopeArgument::First,
    },
];
const LAGRANGE_BINDERS: &[BinderDescriptor] = &[
    BinderDescriptor {
        binder_argument: 2,
        scope_argument: ScopeArgument::First,
    },
    BinderDescriptor {
        binder_argument: 2,
        scope_argument: ScopeArgument::Index(1),
    },
    BinderDescriptor {
        binder_argument: 3,
        scope_argument: ScopeArgument::First,
    },
    BinderDescriptor {
        binder_argument: 3,
        scope_argument: ScopeArgument::Index(1),
    },
];

use Requirement::{
    ApproachPoint, Direction, LowerBound, Operand, Order, Replacement, UpperBound, Variable,
};

const SIG_D: &[OperatorSlotSignature] = &[
    OperatorSlotSignature {
        arity: 2,
        requirements: &[Variable, Operand],
    },
    OperatorSlotSignature {
        arity: 3,
        requirements: &[Variable, Order, Operand],
    },
];
const SIG_UNARY: &[OperatorSlotSignature] = &[OperatorSlotSignature {
    arity: 1,
    requirements: &[Operand],
}];
const SIG_BINARY: &[OperatorSlotSignature] = &[OperatorSlotSignature {
    arity: 2,
    requirements: &[Operand, Operand],
}];
const SIG_INTEGRAL: &[OperatorSlotSignature] = &[
    OperatorSlotSignature {
        arity: 2,
        requirements: &[Variable, Operand],
    },
    OperatorSlotSignature {
        arity: 4,
        requirements: &[Variable, LowerBound, UpperBound, Operand],
    },
];
const SIG_SUBST: &[OperatorSlotSignature] = &[OperatorSlotSignature {
    arity: 3,
    requirements: &[Variable, Replacement, Operand],
}];
const SIG_LIMIT: &[OperatorSlotSignature] = &[
    OperatorSlotSignature {
        arity: 2,
        requirements: &[Operand, ApproachPoint],
    },
    OperatorSlotSignature {
        arity: 3,
        requirements: &[Variable, ApproachPoint, Operand],
    },
    OperatorSlotSignature {
        arity: 4,
        requirements: &[Variable, ApproachPoint, Direction, Operand],
    },
];
const SIG_TAYLOR: &[OperatorSlotSignature] = &[
    OperatorSlotSignature {
        arity: 3,
        requirements: &[Operand, ApproachPoint, Order],
    },
    OperatorSlotSignature {
        arity: 4,
        requirements: &[Variable, ApproachPoint, Order, Operand],
    },
];
const SIG_APPROXIMATE: &[OperatorSlotSignature] = &[
    OperatorSlotSignature {
        arity: 1,
        requirements: &[Operand],
    },
    OperatorSlotSignature {
        arity: 2,
        requirements: &[Operand, Requirement::Precision],
    },
];
const SIG_SUM: &[OperatorSlotSignature] = &[OperatorSlotSignature {
    arity: 4,
    requirements: &[Variable, LowerBound, UpperBound, Operand],
}];
const SIG_DEFINED_INTEGRAL: &[OperatorSlotSignature] = &[
    OperatorSlotSignature {
        arity: 4,
        requirements: &[Operand, Variable, LowerBound, UpperBound],
    },
    OperatorSlotSignature {
        arity: 5,
        requirements: &[Operand, Variable, LowerBound, UpperBound, Operand],
    },
];
const SIG_DOUBLE_INTEGRAL: &[OperatorSlotSignature] = &[OperatorSlotSignature {
    arity: 7,
    requirements: &[
        Operand, Variable, LowerBound, UpperBound, Variable, LowerBound, UpperBound,
    ],
}];
const SIG_POLAR_INTEGRAL: &[OperatorSlotSignature] = &[OperatorSlotSignature {
    arity: 9,
    requirements: &[
        Operand, Variable, Variable, Variable, Variable, LowerBound, UpperBound, LowerBound,
        UpperBound,
    ],
}];
const SIG_NUMERIC_ODE: &[OperatorSlotSignature] = &[OperatorSlotSignature {
    arity: 6,
    requirements: &[
        Operand,
        Variable,
        Variable,
        ApproachPoint,
        Replacement,
        ApproachPoint,
    ],
}];
const SIG_FIND_ROOT: &[OperatorSlotSignature] = &[OperatorSlotSignature {
    arity: 3,
    requirements: &[Operand, Variable, ApproachPoint],
}];
const SIG_PLOT: &[OperatorSlotSignature] = &[OperatorSlotSignature {
    arity: 4,
    requirements: &[Operand, Variable, LowerBound, UpperBound],
}];
const SIG_EXTREMA: &[OperatorSlotSignature] = &[OperatorSlotSignature {
    arity: 3,
    requirements: &[Operand, Variable, Variable],
}];
const SIG_LAGRANGE: &[OperatorSlotSignature] = &[OperatorSlotSignature {
    arity: 4,
    requirements: &[Operand, Operand, Variable, Variable],
}];
const SIG_MULTIVARIATE: &[OperatorSlotSignature] = &[
    OperatorSlotSignature {
        arity: 2,
        requirements: &[Operand, Operand],
    },
    OperatorSlotSignature {
        arity: 3,
        requirements: &[Operand, Operand, Operand],
    },
];
const SIG_DIRECTIONAL_DERIVATIVE: &[OperatorSlotSignature] = &[
    OperatorSlotSignature {
        arity: 3,
        requirements: &[Operand, Operand, Direction],
    },
    OperatorSlotSignature {
        arity: 4,
        requirements: &[Operand, Operand, Direction, Requirement::Assumption],
    },
    OperatorSlotSignature {
        arity: 5,
        requirements: &[
            Operand,
            Operand,
            Direction,
            Requirement::Assumption,
            Operand,
        ],
    },
];
const SIG_LINE_INTEGRAL: &[OperatorSlotSignature] = &[OperatorSlotSignature {
    arity: 6,
    requirements: &[Operand, Operand, Operand, Variable, LowerBound, UpperBound],
}];
const SIG_SURFACE_INTEGRAL: &[OperatorSlotSignature] = &[
    OperatorSlotSignature {
        arity: 6,
        requirements: &[Operand, Operand, Operand, Operand, Operand, Operand],
    },
    OperatorSlotSignature {
        arity: 7,
        requirements: &[
            Operand,
            Operand,
            Operand,
            Operand,
            Operand,
            Operand,
            Requirement::Direction,
        ],
    },
];

pub const OPERATOR_DESCRIPTORS: &[OperatorDescriptor] = &[
    OperatorDescriptor {
        id: OperatorId::Derivative,
        names: &["D", "Deriv"],
        arities: &[2, 3],
        slot_signatures: SIG_D,
        forms: BODIED,
        value_argument: ValueArgument::Last,
        binders: FIRST_ARGUMENT_BINDS_LAST,
        capability: CapabilityId::Differentiate,
        route: ObjectNativeRoute::Calculus,
        execution_handler: crate::operator_handlers::execute_calculus_adapter,
        product_kind: "derivative",
        title: "导数",
    },
    OperatorDescriptor {
        id: OperatorId::Factor,
        names: &["Factor"],
        arities: &[1],
        slot_signatures: SIG_UNARY,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::Factor,
        route: ObjectNativeRoute::AlgebraTransform,
        execution_handler: crate::operator_handlers::execute_algebra_transform_adapter,
        product_kind: "algebra",
        title: "代数变换",
    },
    OperatorDescriptor {
        id: OperatorId::AlgebraTransform,
        names: &["Expand", "Simplify", "Tidy"],
        arities: &[1],
        slot_signatures: SIG_UNARY,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::TransformAlgebra,
        route: ObjectNativeRoute::AlgebraTransform,
        execution_handler: crate::operator_handlers::execute_algebra_transform_adapter,
        product_kind: "algebra",
        title: "代数变换",
    },
    OperatorDescriptor {
        id: OperatorId::AlgebraTransform,
        names: &["Apart"],
        arities: &[2],
        slot_signatures: SIG_BINARY,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::TransformAlgebra,
        route: ObjectNativeRoute::AlgebraTransform,
        execution_handler: crate::operator_handlers::execute_algebra_transform_adapter,
        product_kind: "algebra",
        title: "部分分式分解",
    },
    OperatorDescriptor {
        id: OperatorId::Integral,
        names: &["Integrate"],
        arities: &[2, 4],
        slot_signatures: SIG_INTEGRAL,
        forms: BODIED,
        value_argument: ValueArgument::Last,
        binders: FIRST_ARGUMENT_BINDS_LAST,
        capability: CapabilityId::Integrate,
        route: ObjectNativeRoute::Calculus,
        execution_handler: crate::operator_handlers::execute_calculus_adapter,
        product_kind: "integral",
        title: "积分",
    },
    OperatorDescriptor {
        id: OperatorId::Substitute,
        names: &["Subst"],
        arities: &[3],
        slot_signatures: SIG_SUBST,
        forms: BODIED,
        value_argument: ValueArgument::Last,
        binders: NO_BINDERS,
        capability: CapabilityId::Substitute,
        route: ObjectNativeRoute::Substitute,
        execution_handler: crate::operator_handlers::execute_substitution_adapter,
        product_kind: "substitution",
        title: "变量替换",
    },
    OperatorDescriptor {
        id: OperatorId::Limit,
        names: &["Limit"],
        arities: &[2, 3, 4],
        slot_signatures: SIG_LIMIT,
        forms: BODIED_AND_CONVENTIONAL,
        value_argument: ValueArgument::Last,
        binders: FIRST_ARGUMENT_BINDS_LAST,
        capability: CapabilityId::EvaluateLimit,
        route: ObjectNativeRoute::Calculus,
        execution_handler: crate::operator_handlers::execute_calculus_adapter,
        product_kind: "limit",
        title: "极限",
    },
    OperatorDescriptor {
        id: OperatorId::Taylor,
        names: &["Taylor"],
        arities: &[3, 4],
        slot_signatures: SIG_TAYLOR,
        forms: BODIED_AND_CONVENTIONAL,
        value_argument: ValueArgument::Last,
        binders: FIRST_ARGUMENT_BINDS_LAST,
        capability: CapabilityId::ExpandTaylor,
        route: ObjectNativeRoute::Taylor,
        execution_handler: crate::operator_handlers::execute_taylor_adapter,
        product_kind: "taylor",
        title: "Taylor 多项式",
    },
    OperatorDescriptor {
        id: OperatorId::Solve,
        names: &["Solve"],
        arities: &[2],
        slot_signatures: SIG_BINARY,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: SECOND_ARGUMENT_BINDS_FIRST,
        capability: CapabilityId::SolveEquation,
        route: ObjectNativeRoute::EquationSolve,
        execution_handler: crate::operator_handlers::execute_equation_solve_adapter,
        product_kind: "equation",
        title: "方程",
    },
    OperatorDescriptor {
        id: OperatorId::MatrixTransform,
        names: &["Transpose", "Determinant", "Inverse"],
        arities: &[1],
        slot_signatures: SIG_UNARY,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::TransformMatrix,
        route: ObjectNativeRoute::MatrixUnary,
        execution_handler: crate::operator_handlers::execute_matrix_unary_adapter,
        product_kind: "matrix",
        title: "线性代数",
    },
    OperatorDescriptor {
        id: OperatorId::MatrixSolve,
        names: &["MatrixSolve", "SolveMatrix"],
        arities: &[2],
        slot_signatures: SIG_BINARY,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::TransformMatrix,
        route: ObjectNativeRoute::MatrixSolve,
        execution_handler: crate::operator_handlers::execute_matrix_solve_adapter,
        product_kind: "matrix",
        title: "线性方程组",
    },
    OperatorDescriptor {
        id: OperatorId::MatrixAnalyze,
        names: &[
            "Rank",
            "RREF",
            "RowReduce",
            "EigenValues",
            "NullSpace",
            "ColumnSpace",
            "EigenSpaces",
        ],
        arities: &[1],
        slot_signatures: SIG_UNARY,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::AnalyzeMatrix,
        route: ObjectNativeRoute::MatrixUnary,
        execution_handler: crate::operator_handlers::execute_matrix_unary_adapter,
        product_kind: "matrix",
        title: "线性代数",
    },
    OperatorDescriptor {
        id: OperatorId::MatrixDecompose,
        names: &[
            "PLDU",
            "Cholesky",
            "GramSchmidt",
            "OrthogonalBasis",
            "OrthonormalBasis",
        ],
        arities: &[1],
        slot_signatures: SIG_UNARY,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::AnalyzeMatrix,
        route: ObjectNativeRoute::MatrixUnary,
        execution_handler: crate::operator_handlers::execute_matrix_unary_adapter,
        product_kind: "matrix",
        title: "线性代数",
    },
    OperatorDescriptor {
        id: OperatorId::FactorProjection,
        names: &["Factors"],
        arities: &[1],
        slot_signatures: SIG_UNARY,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::AnalyzeMatrix,
        route: ObjectNativeRoute::FactorProjection,
        execution_handler: crate::operator_handlers::execute_factor_projection_adapter,
        product_kind: "matrix",
        title: "矩阵因子",
    },
    OperatorDescriptor {
        id: OperatorId::OdeSolve,
        names: &["OdeSolve"],
        arities: &[1],
        slot_signatures: SIG_UNARY,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::SolveOde,
        route: ObjectNativeRoute::OdeSolve,
        execution_handler: crate::operator_handlers::execute_ode_solve_adapter,
        product_kind: "ode",
        title: "常微分方程",
    },
    OperatorDescriptor {
        id: OperatorId::Approximate,
        names: &["N", "Approximate"],
        arities: &[1, 2],
        slot_signatures: SIG_APPROXIMATE,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::Approximate,
        route: ObjectNativeRoute::Approximate,
        execution_handler: crate::operator_handlers::execute_approximate_adapter,
        product_kind: "numeric",
        title: "数值近似",
    },
    OperatorDescriptor {
        id: OperatorId::Sum,
        names: &["Sum"],
        arities: &[4],
        slot_signatures: SIG_SUM,
        forms: CALL,
        value_argument: ValueArgument::Last,
        binders: FIRST_ARGUMENT_BINDS_LAST,
        capability: CapabilityId::SumSeries,
        route: ObjectNativeRoute::Series,
        execution_handler: crate::operator_handlers::execute_series_adapter,
        product_kind: "series",
        title: "级数",
    },
    OperatorDescriptor {
        id: OperatorId::ImproperIntegral,
        names: &["ImproperIntegral"],
        arities: &[4, 5],
        slot_signatures: SIG_DEFINED_INTEGRAL,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: SECOND_ARGUMENT_BINDS_FIRST,
        capability: CapabilityId::IntegrateDefined,
        route: ObjectNativeRoute::DefinedIntegral,
        execution_handler: crate::operator_handlers::execute_improper_integral_adapter,
        product_kind: "defined_object",
        title: "反常积分",
    },
    OperatorDescriptor {
        id: OperatorId::PrincipalValueIntegral,
        names: &["PrincipalValueIntegral"],
        arities: &[4, 5],
        slot_signatures: SIG_DEFINED_INTEGRAL,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: SECOND_ARGUMENT_BINDS_FIRST,
        capability: CapabilityId::IntegrateDefined,
        route: ObjectNativeRoute::DefinedIntegral,
        execution_handler: crate::operator_handlers::execute_principal_value_integral_adapter,
        product_kind: "defined_object",
        title: "Cauchy 主值",
    },
    OperatorDescriptor {
        id: OperatorId::DoubleIntegral,
        names: &["DoubleIntegral"],
        arities: &[7],
        slot_signatures: SIG_DOUBLE_INTEGRAL,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: DOUBLE_INTEGRAL_BINDERS,
        capability: CapabilityId::IntegrateMultiple,
        route: ObjectNativeRoute::MultipleIntegral,
        execution_handler: crate::operator_handlers::execute_double_integral_adapter,
        product_kind: "double_integral",
        title: "二重积分",
    },
    OperatorDescriptor {
        id: OperatorId::PolarIntegral,
        names: &["PolarIntegral"],
        arities: &[9],
        slot_signatures: SIG_POLAR_INTEGRAL,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: SECOND_AND_THIRD_BIND_FIRST,
        capability: CapabilityId::IntegrateMultiple,
        route: ObjectNativeRoute::MultipleIntegral,
        execution_handler: crate::operator_handlers::execute_polar_integral_adapter,
        product_kind: "polar_integral",
        title: "极坐标积分",
    },
    OperatorDescriptor {
        id: OperatorId::OdeSolveNumeric,
        names: &["OdeSolveNumeric"],
        arities: &[6],
        slot_signatures: SIG_NUMERIC_ODE,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::SolveNumericOde,
        route: ObjectNativeRoute::NumericOde,
        execution_handler: crate::operator_handlers::execute_numeric_ode_adapter,
        product_kind: "numeric_ode",
        title: "常微分方程数值解",
    },
    OperatorDescriptor {
        id: OperatorId::FindRoot,
        names: &["FindRoot"],
        arities: &[3],
        slot_signatures: SIG_FIND_ROOT,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: SECOND_ARGUMENT_BINDS_FIRST,
        capability: CapabilityId::FindNumericRoot,
        route: ObjectNativeRoute::NumericRoot,
        execution_handler: crate::operator_handlers::execute_numeric_root_adapter,
        product_kind: "numeric_root",
        title: "数值根",
    },
    OperatorDescriptor {
        id: OperatorId::Plot,
        names: &["Plot"],
        arities: &[4],
        slot_signatures: SIG_PLOT,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: SECOND_ARGUMENT_BINDS_FIRST,
        capability: CapabilityId::RenderPlot,
        route: ObjectNativeRoute::PlotEffect,
        execution_handler: crate::operator_handlers::execute_plot_effect_adapter,
        product_kind: "plot",
        title: "函数图像",
    },
    OperatorDescriptor {
        id: OperatorId::Extrema,
        names: &["Extrema"],
        arities: &[3],
        slot_signatures: SIG_EXTREMA,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: SECOND_AND_THIRD_BIND_FIRST,
        capability: CapabilityId::AnalyzeExtrema,
        route: ObjectNativeRoute::Extrema,
        execution_handler: crate::operator_handlers::execute_extrema_adapter,
        product_kind: "extrema",
        title: "无约束极值",
    },
    OperatorDescriptor {
        id: OperatorId::Lagrange,
        names: &["Lagrange"],
        arities: &[4],
        slot_signatures: SIG_LAGRANGE,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: LAGRANGE_BINDERS,
        capability: CapabilityId::AnalyzeExtrema,
        route: ObjectNativeRoute::Extrema,
        execution_handler: crate::operator_handlers::execute_lagrange_adapter,
        product_kind: "lagrange",
        title: "约束极值",
    },
    OperatorDescriptor {
        id: OperatorId::MultivariateDifferential,
        names: &["Gradient", "Jacobian", "Hessian", "Divergence", "Curl"],
        arities: &[2, 3],
        slot_signatures: SIG_MULTIVARIATE,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::AnalyzeMultivariate,
        route: ObjectNativeRoute::MultivariateDifferential,
        execution_handler: crate::operator_handlers::execute_multivariate_adapter,
        product_kind: "multivariate",
        title: "多元微分",
    },
    OperatorDescriptor {
        id: OperatorId::MultivariateDifferential,
        names: &["DirectionalDerivative"],
        arities: &[3, 4, 5],
        slot_signatures: SIG_DIRECTIONAL_DERIVATIVE,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::AnalyzeMultivariate,
        route: ObjectNativeRoute::MultivariateDifferential,
        execution_handler: crate::operator_handlers::execute_multivariate_adapter,
        product_kind: "multivariate",
        title: "方向导数",
    },
    OperatorDescriptor {
        id: OperatorId::LineIntegral,
        names: &["ScalarLineIntegral", "VectorLineIntegral"],
        arities: &[6],
        slot_signatures: SIG_LINE_INTEGRAL,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::IntegrateLine,
        route: ObjectNativeRoute::LineIntegral,
        execution_handler: crate::operator_handlers::execute_line_integral_adapter,
        product_kind: "line_integral",
        title: "线积分",
    },
    OperatorDescriptor {
        id: OperatorId::SurfaceIntegral,
        names: &["ScalarSurfaceIntegral", "VectorSurfaceIntegral"],
        arities: &[6, 7],
        slot_signatures: SIG_SURFACE_INTEGRAL,
        forms: CALL,
        value_argument: ValueArgument::First,
        binders: NO_BINDERS,
        capability: CapabilityId::IntegrateSurface,
        route: ObjectNativeRoute::SurfaceIntegral,
        execution_handler: crate::operator_handlers::execute_surface_integral_adapter,
        product_kind: "surface_integral",
        title: "曲面积分",
    },
];

pub fn operator_descriptor(name: &str) -> Option<&'static OperatorDescriptor> {
    OPERATOR_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.names.contains(&name))
}

pub fn is_known_operator(name: &str) -> bool {
    operator_descriptor(name).is_some()
}

pub fn object_native_route(name: &str) -> Option<ObjectNativeRoute> {
    Some(operator_descriptor(name)?.route)
}

pub fn is_object_native_operator(name: &str) -> bool {
    object_native_route(name).is_some()
}
