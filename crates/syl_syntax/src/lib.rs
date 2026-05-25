//! Syntax front-end for Syl.
//!
//! `syl_syntax` keeps syntax concerns split across modules:
//! - `ast`: typed AST data structures
//! - `token` and `lexer`: token definitions and lexing mechanics
//! - `parser`: parsing, diagnostics, and recovery
//! - `lossless`: trivia-preserving syntax tree for formatter/LSP workflows

mod ast;
mod build;
mod dump;
mod lossless;
mod node_index;

pub mod lexer;
pub mod parser;
pub mod token;

pub use ast::{
    AstFile, Attribute, BinaryOp, Block, BundleItem, CallArg, CallableItem, ConstItem,
    DriveCapability, EnumItem, EnumVariant, ErrorItem, Expr, ExternModuleItem, FieldDecl, FnItem,
    GenericParam, InterfaceItem, Item, MapItem, MatchArm, NamedExpr, PackageItem, Param,
    ParamDirection, Pattern, PortDecl, RegReset, ResultBinding, SelectArm, SelectMode, Stmt,
    TypeExpr, UnaryOp, UseItem, ViewDecl, ViewDirection, ViewField,
};
pub use lossless::{LosslessItemKind, LosslessNodeKind, LosslessSyntaxElement, LosslessSyntaxNode};
pub use lossless::{LosslessSyntaxFile, LosslessToken, LosslessTokenKind};
pub use node_index::{AstNodeId, AstNodeIndex, AstNodeKind, AstNodeRecord};
pub use parser::{ParseOutput, SourceParser};
