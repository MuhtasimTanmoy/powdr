pub mod asm;
pub mod build;
pub mod display;
pub mod folder;
pub mod types;
pub mod utils;
pub mod visitor;

use std::{
    collections::BTreeSet,
    iter::{empty, once},
    ops,
};

use powdr_number::{BigUint, DegreeType};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use self::{
    asm::{Part, SymbolPath},
    types::{FunctionType, Type, TypeScheme},
    visitor::Children,
};
use crate::SourceRef;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PILFile(pub Vec<PilStatement>);

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum PilStatement {
    /// File name
    Include(SourceRef, String),
    /// Name of namespace and polynomial degree (constant)
    Namespace(SourceRef, SymbolPath, Expression),
    LetStatement(
        SourceRef,
        String,
        Option<TypeScheme<Expression>>,
        Option<Expression>,
    ),
    PolynomialDefinition(SourceRef, String, Expression),
    PublicDeclaration(
        SourceRef,
        /// The name of the public value.
        String,
        /// The polynomial/column that contains the public value.
        NamespacedPolynomialReference,
        /// If the polynomial is an array, this is the array element index.
        Option<Expression>,
        /// The row number of the public value.
        Expression,
    ),
    PolynomialConstantDeclaration(SourceRef, Vec<PolynomialName>),
    PolynomialConstantDefinition(SourceRef, String, FunctionDefinition),
    PolynomialCommitDeclaration(
        SourceRef,
        // Stage
        Option<u32>,
        // Names
        Vec<PolynomialName>,
        // Value (prover query / hint)
        Option<FunctionDefinition>,
    ),
    PlookupIdentity(
        SourceRef,
        SelectedExpressions<Expression>,
        SelectedExpressions<Expression>,
    ),
    PermutationIdentity(
        SourceRef,
        SelectedExpressions<Expression>,
        SelectedExpressions<Expression>,
    ),
    ConnectIdentity(SourceRef, Vec<Expression>, Vec<Expression>),
    ConstantDefinition(SourceRef, String, Expression),
    EnumDeclaration(SourceRef, EnumDeclaration<Expression>),
    Expression(SourceRef, Expression),
}

impl PilStatement {
    /// If the statement is a symbol definition, returns all (local) names of defined symbols.
    /// Note it does not return nested definitions (for an enum for example).
    /// The boolean indicates if the name is a type definition or a value definition.
    pub fn symbol_definition_names(&self) -> Box<dyn Iterator<Item = (&String, bool)> + '_> {
        match self {
            PilStatement::PolynomialDefinition(_, name, _)
            | PilStatement::PolynomialConstantDefinition(_, name, _)
            | PilStatement::ConstantDefinition(_, name, _)
            | PilStatement::PublicDeclaration(_, name, _, _, _)
            | PilStatement::LetStatement(_, name, _, _) => Box::new(once((name, false))),
            PilStatement::EnumDeclaration(_, EnumDeclaration { name, variants: _ }) => {
                Box::new(once((name, true)))
            }
            PilStatement::PolynomialConstantDeclaration(_, polynomials)
            | PilStatement::PolynomialCommitDeclaration(_, _, polynomials, _) => {
                Box::new(polynomials.iter().map(|p| (&p.name, false)))
            }

            PilStatement::Include(_, _)
            | PilStatement::Namespace(_, _, _)
            | PilStatement::PlookupIdentity(_, _, _)
            | PilStatement::PermutationIdentity(_, _, _)
            | PilStatement::ConnectIdentity(_, _, _)
            | PilStatement::Expression(_, _) => Box::new(empty()),
        }
    }

    /// If the statement defines any symbols inside a namespace, returns
    /// the name of the namespace and defined names inside that namespace.
    /// The boolean indicates if the name is a type definition or a value definition.
    pub fn defined_contained_names(
        &self,
    ) -> Box<dyn Iterator<Item = (&String, &String, bool)> + '_> {
        match self {
            PilStatement::EnumDeclaration(_, EnumDeclaration { name, variants }) => {
                Box::new(variants.iter().map(move |v| (name, &v.name, false)))
            }
            _ => Box::new(empty()),
        }
    }
}

impl Children<Expression> for PilStatement {
    /// Returns an iterator over all (top-level) expressions in this statement.
    fn children(&self) -> Box<dyn Iterator<Item = &Expression> + '_> {
        match self {
            PilStatement::PlookupIdentity(_, left, right)
            | PilStatement::PermutationIdentity(_, left, right) => {
                Box::new(left.children().chain(right.children()))
            }
            PilStatement::ConnectIdentity(_start, left, right) => {
                Box::new(left.iter().chain(right.iter()))
            }
            PilStatement::Expression(_, e)
            | PilStatement::Namespace(_, _, e)
            | PilStatement::PolynomialDefinition(_, _, e)
            | PilStatement::ConstantDefinition(_, _, e) => Box::new(once(e)),

            PilStatement::EnumDeclaration(_, enum_decl) => enum_decl.children(),

            PilStatement::LetStatement(_, _, type_scheme, value) => Box::new(
                type_scheme
                    .iter()
                    .flat_map(|t| t.ty.children())
                    .chain(value),
            ),

            PilStatement::PublicDeclaration(_, _, _, i, e) => Box::new(i.iter().chain(once(e))),

            PilStatement::PolynomialConstantDefinition(_, _, def)
            | PilStatement::PolynomialCommitDeclaration(_, _, _, Some(def)) => def.children(),
            PilStatement::PolynomialCommitDeclaration(_, _, _, None)
            | PilStatement::Include(_, _)
            | PilStatement::PolynomialConstantDeclaration(_, _) => Box::new(empty()),
        }
    }

    /// Returns an iterator over all (top-level) expressions in this statement.
    fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expression> + '_> {
        match self {
            PilStatement::PlookupIdentity(_, left, right)
            | PilStatement::PermutationIdentity(_, left, right) => {
                Box::new(left.children_mut().chain(right.children_mut()))
            }
            PilStatement::ConnectIdentity(_start, left, right) => {
                Box::new(left.iter_mut().chain(right.iter_mut()))
            }
            PilStatement::Expression(_, e)
            | PilStatement::Namespace(_, _, e)
            | PilStatement::PolynomialDefinition(_, _, e)
            | PilStatement::ConstantDefinition(_, _, e) => Box::new(once(e)),

            PilStatement::EnumDeclaration(_, enum_decl) => enum_decl.children_mut(),

            PilStatement::LetStatement(_, _, ty, value) => {
                Box::new(ty.iter_mut().flat_map(|t| t.ty.children_mut()).chain(value))
            }

            PilStatement::PublicDeclaration(_, _, _, i, e) => Box::new(i.iter_mut().chain(once(e))),

            PilStatement::PolynomialConstantDefinition(_, _, def)
            | PilStatement::PolynomialCommitDeclaration(_, _, _, Some(def)) => def.children_mut(),
            PilStatement::PolynomialCommitDeclaration(_, _, _, None)
            | PilStatement::Include(_, _)
            | PilStatement::PolynomialConstantDeclaration(_, _) => Box::new(empty()),
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EnumDeclaration<E = u64> {
    pub name: String,
    pub variants: Vec<EnumVariant<E>>,
}

impl<R> Children<Expression<R>> for EnumDeclaration<u64> {
    fn children(&self) -> Box<dyn Iterator<Item = &Expression<R>> + '_> {
        Box::new(empty())
    }
    fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expression<R>> + '_> {
        Box::new(empty())
    }
}

impl<R> Children<Expression<R>> for EnumDeclaration<Expression<R>> {
    fn children(&self) -> Box<dyn Iterator<Item = &Expression<R>> + '_> {
        Box::new(self.variants.iter().flat_map(|v| v.children()))
    }
    fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expression<R>> + '_> {
        Box::new(self.variants.iter_mut().flat_map(|v| v.children_mut()))
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EnumVariant<E = u64> {
    pub name: String,
    pub fields: Option<Vec<Type<E>>>,
}

impl<E: Clone> EnumVariant<E> {
    /// Returns the type of the constructor function for this variant
    /// given the name of the enum type.
    pub fn constructor_type(&self, type_name: SymbolPath) -> Type<E> {
        match &self.fields {
            None => Type::NamedType(type_name),
            Some(fields) => Type::Function(FunctionType {
                params: (*fields).clone(),
                value: Type::NamedType(type_name).into(),
            }),
        }
    }
}

impl<R> Children<Expression<R>> for EnumVariant<u64> {
    fn children(&self) -> Box<dyn Iterator<Item = &Expression<R>> + '_> {
        Box::new(empty())
    }
    fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expression<R>> + '_> {
        Box::new(empty())
    }
}

impl<R> Children<Expression<R>> for EnumVariant<Expression<R>> {
    fn children(&self) -> Box<dyn Iterator<Item = &Expression<R>> + '_> {
        Box::new(
            self.fields
                .iter()
                .flat_map(|f| f.iter())
                .flat_map(|f| f.children()),
        )
    }
    fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expression<R>> + '_> {
        Box::new(
            self.fields
                .iter_mut()
                .flat_map(|f| f.iter_mut())
                .flat_map(|f| f.children_mut()),
        )
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SelectedExpressions<Expr> {
    pub selector: Option<Expr>,
    pub expressions: Vec<Expr>,
}

impl<Expr> Default for SelectedExpressions<Expr> {
    fn default() -> Self {
        Self {
            selector: Default::default(),
            expressions: Default::default(),
        }
    }
}

impl<Expr> Children<Expr> for SelectedExpressions<Expr> {
    /// Returns an iterator over all (top-level) expressions in this SelectedExpressions.
    fn children(&self) -> Box<dyn Iterator<Item = &Expr> + '_> {
        Box::new(self.selector.iter().chain(self.expressions.iter()))
    }

    /// Returns an iterator over all (top-level) expressions in this SelectedExpressions.
    fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expr> + '_> {
        Box::new(self.selector.iter_mut().chain(self.expressions.iter_mut()))
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize, JsonSchema)]
pub enum Expression<Ref = NamespacedPolynomialReference> {
    Reference(Ref),
    PublicReference(String),
    // A number literal and its type.
    Number(#[schemars(skip)] BigUint, Option<Type>),
    String(String),
    Tuple(Vec<Expression<Ref>>),
    LambdaExpression(LambdaExpression<Ref>),
    ArrayLiteral(ArrayLiteral<Ref>),
    BinaryOperation(Box<Expression<Ref>>, BinaryOperator, Box<Expression<Ref>>),
    UnaryOperation(UnaryOperator, Box<Expression<Ref>>),
    IndexAccess(IndexAccess<Ref>),
    FunctionCall(FunctionCall<Ref>),
    FreeInput(Box<Expression<Ref>>),
    MatchExpression(Box<Expression<Ref>>, Vec<MatchArm<Ref>>),
    IfExpression(IfExpression<Ref>),
    BlockExpression(Vec<StatementInsideBlock<Ref>>, Box<Expression<Ref>>),
}

impl<Ref> Expression<Ref> {
    pub fn new_binary(left: Self, op: BinaryOperator, right: Self) -> Self {
        Expression::BinaryOperation(Box::new(left), op, Box::new(right))
    }

    /// Visits this expression and all of its sub-expressions and returns true
    /// if `f` returns true on any of them.
    pub fn any(&self, mut f: impl FnMut(&Self) -> bool) -> bool {
        use std::ops::ControlFlow;
        use visitor::ExpressionVisitable;
        self.pre_visit_expressions_return(&mut |e| {
            if f(e) {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .is_break()
    }
}

impl From<u32> for Expression {
    fn from(value: u32) -> Self {
        Expression::Number(value.into(), None)
    }
}

impl From<BigUint> for Expression {
    fn from(value: BigUint) -> Self {
        Expression::Number(value, None)
    }
}

impl<Ref> ops::Add for Expression<Ref> {
    type Output = Expression<Ref>;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new_binary(self, BinaryOperator::Add, rhs)
    }
}

impl<Ref> ops::Sub for Expression<Ref> {
    type Output = Expression<Ref>;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new_binary(self, BinaryOperator::Sub, rhs)
    }
}
impl<Ref> ops::Mul for Expression<Ref> {
    type Output = Expression<Ref>;

    fn mul(self, rhs: Self) -> Self::Output {
        Self::new_binary(self, BinaryOperator::Mul, rhs)
    }
}

impl<Ref> std::iter::Sum for Expression<Ref> {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.reduce(|a, b| a + b)
            .unwrap_or_else(|| Expression::Number(0u32.into(), None))
    }
}

impl From<NamespacedPolynomialReference> for Expression {
    fn from(value: NamespacedPolynomialReference) -> Self {
        Self::Reference(value)
    }
}

impl<R> Expression<R> {
    /// Returns an iterator over all (top-level) expressions in this expression.
    /// This specifically does not implement Children so that we can implement
    /// ExpressionVisitable generically.
    pub fn children(&self) -> Box<dyn Iterator<Item = &Expression<R>> + '_> {
        match self {
            Expression::Reference(_) | Expression::PublicReference(_) | Expression::String(_) => {
                Box::new(empty())
            }
            Expression::Number(_, _) => Box::new(empty()),
            Expression::Tuple(v) => Box::new(v.iter()),
            Expression::LambdaExpression(LambdaExpression { body, .. }) => {
                Box::new(once(body.as_ref()))
            }
            Expression::ArrayLiteral(ArrayLiteral { items }) => Box::new(items.iter()),
            Expression::BinaryOperation(left, _, right) => {
                Box::new([left.as_ref(), right.as_ref()].into_iter())
            }
            Expression::UnaryOperation(_, e) => Box::new(once(e.as_ref())),
            Expression::IndexAccess(IndexAccess { array, index }) => {
                Box::new([array.as_ref(), index.as_ref()].into_iter())
            }
            Expression::FunctionCall(FunctionCall {
                function,
                arguments,
            }) => Box::new(once(function.as_ref()).chain(arguments.iter())),
            Expression::FreeInput(e) => Box::new(once(e.as_ref())),
            Expression::MatchExpression(e, arms) => {
                Box::new(once(e.as_ref()).chain(arms.iter().flat_map(|arm| arm.children())))
            }
            Expression::IfExpression(IfExpression {
                condition,
                body,
                else_body,
            }) => Box::new([condition, body, else_body].into_iter().map(|e| e.as_ref())),
            Expression::BlockExpression(statements, expr) => Box::new(
                statements
                    .iter()
                    .flat_map(|s| s.children())
                    .chain(once(expr.as_ref())),
            ),
        }
    }

    /// Returns an iterator over all (top-level) expressions in this expression.
    /// This specifically does not implement Children so that we can implement
    /// ExpressionVisitable generically.
    pub fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expression<R>> + '_> {
        match self {
            Expression::Reference(_) | Expression::PublicReference(_) | Expression::String(_) => {
                Box::new(empty())
            }
            Expression::Number(_, _) => Box::new(empty()),
            Expression::Tuple(v) => Box::new(v.iter_mut()),
            Expression::LambdaExpression(LambdaExpression { body, .. }) => {
                Box::new(once(body.as_mut()))
            }
            Expression::ArrayLiteral(ArrayLiteral { items }) => Box::new(items.iter_mut()),
            Expression::BinaryOperation(left, _, right) => {
                Box::new([left.as_mut(), right.as_mut()].into_iter())
            }
            Expression::UnaryOperation(_, e) => Box::new(once(e.as_mut())),
            Expression::IndexAccess(IndexAccess { array, index }) => {
                Box::new([array.as_mut(), index.as_mut()].into_iter())
            }
            Expression::FunctionCall(FunctionCall {
                function,
                arguments,
            }) => Box::new(once(function.as_mut()).chain(arguments.iter_mut())),
            Expression::FreeInput(e) => Box::new(once(e.as_mut())),
            Expression::MatchExpression(e, arms) => {
                Box::new(once(e.as_mut()).chain(arms.iter_mut().flat_map(|arm| arm.children_mut())))
            }
            Expression::IfExpression(IfExpression {
                condition,
                body,
                else_body,
            }) => Box::new([condition, body, else_body].into_iter().map(|e| e.as_mut())),
            Expression::BlockExpression(statements, expr) => Box::new(
                statements
                    .iter_mut()
                    .flat_map(|s| s.children_mut())
                    .chain(once(expr.as_mut())),
            ),
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Default, Clone)]
pub struct PolynomialName {
    pub name: String,
    pub array_size: Option<Expression>,
}

#[derive(Debug, PartialEq, Eq, Default, Clone, PartialOrd, Ord)]
/// A polynomial with an optional namespace
/// This is different from SymbolPath mainly due to different formatting.
pub struct NamespacedPolynomialReference {
    pub path: SymbolPath,
}

impl From<SymbolPath> for NamespacedPolynomialReference {
    fn from(value: SymbolPath) -> Self {
        Self { path: value }
    }
}

impl NamespacedPolynomialReference {
    pub fn from_identifier(name: String) -> Self {
        SymbolPath::from_parts(vec![Part::Named(name)]).into()
    }

    pub fn try_to_identifier(&self) -> Option<&String> {
        self.path.try_to_identifier()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct LambdaExpression<Ref = NamespacedPolynomialReference> {
    pub kind: FunctionKind,
    pub params: Vec<String>,
    pub body: Box<Expression<Ref>>,
}

impl<R> Children<Expression<R>> for LambdaExpression<R> {
    fn children(&self) -> Box<dyn Iterator<Item = &Expression<R>> + '_> {
        Box::new(once(self.body.as_ref()))
    }

    fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expression<R>> + '_> {
        Box::new(once(self.body.as_mut()))
    }
}

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub enum FunctionKind {
    Pure,
    Constr,
    Query,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct ArrayLiteral<Ref = NamespacedPolynomialReference> {
    pub items: Vec<Expression<Ref>>,
}

impl<R> Children<Expression<R>> for ArrayLiteral<R> {
    fn children(&self) -> Box<dyn Iterator<Item = &Expression<R>> + '_> {
        Box::new(self.items.iter())
    }

    fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expression<R>> + '_> {
        Box::new(self.items.iter_mut())
    }
}

#[derive(
    Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum UnaryOperator {
    Minus,
    LogicalNot,
    Next,
}

impl UnaryOperator {
    /// Returns true if the operator is a prefix-operator and false if it is a postfix operator.
    pub fn is_prefix(&self) -> bool {
        match self {
            UnaryOperator::Minus | UnaryOperator::LogicalNot => true,
            UnaryOperator::Next => false,
        }
    }
}

#[derive(
    Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum BinaryOperator {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    BinaryAnd,
    BinaryXor,
    BinaryOr,
    ShiftLeft,
    ShiftRight,
    LogicalOr,
    LogicalAnd,
    Less,
    LessEqual,
    Equal,
    Identity,
    NotEqual,
    GreaterEqual,
    Greater,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IndexAccess<Ref = NamespacedPolynomialReference> {
    pub array: Box<Expression<Ref>>,
    pub index: Box<Expression<Ref>>,
}

impl<R> Children<Expression<R>> for IndexAccess<R> {
    fn children(&self) -> Box<dyn Iterator<Item = &Expression<R>> + '_> {
        Box::new(once(self.array.as_ref()).chain(once(self.index.as_ref())))
    }

    fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expression<R>> + '_> {
        Box::new(once(self.array.as_mut()).chain(once(self.index.as_mut())))
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FunctionCall<Ref = NamespacedPolynomialReference> {
    pub function: Box<Expression<Ref>>,
    pub arguments: Vec<Expression<Ref>>,
}

impl<R> Children<Expression<R>> for FunctionCall<R> {
    fn children(&self) -> Box<dyn Iterator<Item = &Expression<R>> + '_> {
        Box::new(once(self.function.as_ref()).chain(self.arguments.iter()))
    }

    fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expression<R>> + '_> {
        Box::new(once(self.function.as_mut()).chain(self.arguments.iter_mut()))
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MatchArm<Ref = NamespacedPolynomialReference> {
    pub pattern: MatchPattern<Ref>,
    pub value: Expression<Ref>,
}

impl<Ref> Children<Expression<Ref>> for MatchArm<Ref> {
    fn children(&self) -> Box<dyn Iterator<Item = &Expression<Ref>> + '_> {
        Box::new(self.pattern.children().chain(once(&self.value)))
    }

    fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expression<Ref>> + '_> {
        Box::new(self.pattern.children_mut().chain(once(&mut self.value)))
    }
}

/// A pattern for a match arm. We could extend this in the future.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize, JsonSchema)]
pub enum MatchPattern<Ref = NamespacedPolynomialReference> {
    CatchAll,
    Pattern(Expression<Ref>),
}

impl<Ref> Children<Expression<Ref>> for MatchPattern<Ref> {
    fn children(&self) -> Box<dyn Iterator<Item = &Expression<Ref>> + '_> {
        Box::new(
            match self {
                MatchPattern::CatchAll => None,
                MatchPattern::Pattern(e) => Some(e),
            }
            .into_iter(),
        )
    }
    fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expression<Ref>> + '_> {
        Box::new(
            match self {
                MatchPattern::CatchAll => None,
                MatchPattern::Pattern(e) => Some(e),
            }
            .into_iter(),
        )
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IfExpression<Ref = NamespacedPolynomialReference> {
    pub condition: Box<Expression<Ref>>,
    pub body: Box<Expression<Ref>>,
    pub else_body: Box<Expression<Ref>>,
}

impl<R> Children<Expression<R>> for IfExpression<R> {
    fn children(&self) -> Box<dyn Iterator<Item = &Expression<R>> + '_> {
        Box::new(
            once(&self.condition)
                .chain(once(&self.body))
                .chain(once(&self.else_body))
                .map(|e| e.as_ref()),
        )
    }

    fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expression<R>> + '_> {
        Box::new(
            once(&mut self.condition)
                .chain(once(&mut self.body))
                .chain(once(&mut self.else_body))
                .map(|e| e.as_mut()),
        )
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize, JsonSchema)]
pub enum StatementInsideBlock<Ref = NamespacedPolynomialReference> {
    LetStatement(LetStatementInsideBlock<Ref>),
    Expression(Expression<Ref>),
}

impl<R> Children<Expression<R>> for StatementInsideBlock<R> {
    fn children(&self) -> Box<dyn Iterator<Item = &Expression<R>> + '_> {
        match self {
            StatementInsideBlock::LetStatement(l) => Box::new(l.children()),
            StatementInsideBlock::Expression(e) => Box::new(once(e)),
        }
    }

    fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expression<R>> + '_> {
        match self {
            StatementInsideBlock::LetStatement(l) => Box::new(l.children_mut()),
            StatementInsideBlock::Expression(e) => Box::new(once(e)),
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LetStatementInsideBlock<Ref = NamespacedPolynomialReference> {
    pub name: String,
    pub value: Option<Expression<Ref>>,
}

impl<R> Children<Expression<R>> for LetStatementInsideBlock<R> {
    fn children(&self) -> Box<dyn Iterator<Item = &Expression<R>> + '_> {
        Box::new(self.value.iter())
    }

    fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expression<R>> + '_> {
        Box::new(self.value.iter_mut())
    }
}

/// The definition of a function (excluding its name):
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum FunctionDefinition {
    /// Array expression.
    Array(ArrayExpression),
    /// Prover query. The Expression usually is a LambdaExpression.
    /// TODO can we replace this by a query-marked lambda expcession?
    Query(Expression),
    /// Generic expression
    Expression(Expression),
    /// A type declaration.
    TypeDeclaration(EnumDeclaration<Expression>),
}

impl Children<Expression> for FunctionDefinition {
    fn children(&self) -> Box<dyn Iterator<Item = &Expression> + '_> {
        match self {
            FunctionDefinition::Array(ae) => ae.children(),
            FunctionDefinition::Query(e) | FunctionDefinition::Expression(e) => Box::new(once(e)),
            FunctionDefinition::TypeDeclaration(_enum_declaration) => todo!(),
        }
    }

    fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expression> + '_> {
        match self {
            FunctionDefinition::Array(ae) => ae.children_mut(),
            FunctionDefinition::Query(e) | FunctionDefinition::Expression(e) => Box::new(once(e)),
            FunctionDefinition::TypeDeclaration(_enum_declaration) => todo!(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum ArrayExpression {
    Value(Vec<Expression>),
    RepeatedValue(Vec<Expression>),
    Concat(Box<ArrayExpression>, Box<ArrayExpression>),
}

impl ArrayExpression {
    pub fn value(v: Vec<Expression>) -> Self {
        Self::Value(v)
    }

    pub fn repeated_value(v: Vec<Expression>) -> Self {
        Self::RepeatedValue(v)
    }

    pub fn concat(self, other: Self) -> Self {
        Self::Concat(Box::new(self), Box::new(other))
    }

    fn pad_with(self, pad: Expression) -> Self {
        Self::concat(self, Self::repeated_value(vec![pad]))
    }

    pub fn pad_with_zeroes(self) -> Self {
        self.pad_with(Expression::Number(0u32.into(), None))
    }

    fn last(&self) -> Option<&Expression> {
        match self {
            ArrayExpression::Value(v) => v.last(),
            ArrayExpression::RepeatedValue(v) => v.last(),
            ArrayExpression::Concat(_, right) => right.last(),
        }
    }

    // return None if `self` is empty
    pub fn pad_with_last(self) -> Option<Self> {
        self.last().cloned().map(|last| self.pad_with(last))
    }
}

impl ArrayExpression {
    /// solve for `*`
    pub fn solve(&self, degree: DegreeType) -> DegreeType {
        assert!(
            self.number_of_repetitions() <= 1,
            "`*` can be used only once in rhs of array definition"
        );
        let len = self.constant_length();
        assert!(
            len <= degree,
            "Array literal is too large ({len}) for degree ({degree})."
        );
        // Fill up the remaining space with the repeated array
        degree - len
    }

    /// The number of times the `*` operator is used
    fn number_of_repetitions(&self) -> usize {
        match self {
            ArrayExpression::RepeatedValue(_) => 1,
            ArrayExpression::Value(_) => 0,
            ArrayExpression::Concat(left, right) => {
                left.number_of_repetitions() + right.number_of_repetitions()
            }
        }
    }

    /// The combined length of the constant-size parts of the array expression.
    fn constant_length(&self) -> DegreeType {
        match self {
            ArrayExpression::RepeatedValue(_) => 0,
            ArrayExpression::Value(e) => e.len() as DegreeType,
            ArrayExpression::Concat(left, right) => {
                left.constant_length() + right.constant_length()
            }
        }
    }
}

impl Children<Expression> for ArrayExpression {
    fn children(&self) -> Box<dyn Iterator<Item = &Expression> + '_> {
        match self {
            ArrayExpression::Value(v) | ArrayExpression::RepeatedValue(v) => Box::new(v.iter()),
            ArrayExpression::Concat(left, right) => {
                Box::new(left.children().chain(right.children()))
            }
        }
    }

    fn children_mut(&mut self) -> Box<dyn Iterator<Item = &mut Expression> + '_> {
        match self {
            ArrayExpression::Value(v) | ArrayExpression::RepeatedValue(v) => Box::new(v.iter_mut()),
            ArrayExpression::Concat(left, right) => {
                Box::new(left.children_mut().chain(right.children_mut()))
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TypedExpression<Ref = NamespacedPolynomialReference, E = Expression<Ref>> {
    pub e: Expression<Ref>,
    pub type_scheme: Option<TypeScheme<E>>,
}
