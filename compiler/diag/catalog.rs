use crate::diag::{DiagSeverity, DiagStage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParseDiagKind {
    Lexical,
    SyntaxError,
    UnexpectedToken,
    ExpectedToken,
    ExpectedStatementBoundary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValidationDiagKind {
    AstRule,
    InvalidLiteral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectDiagKind {
    LoadError,
    Parse(ParseDiagKind),
    Validate(ValidationDiagKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MirDiagKind {
    MissingTerminator,
    AwaitSuspendableMismatch,
    PhiOrder,
    PhiSourceMismatch,
    DefiniteInit,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagDescriptor {
    pub diag_id: &'static str,
    pub code: &'static str,
    pub stage: DiagStage,
    pub severity_default: DiagSeverity,
    pub help_template: &'static str,
}

pub fn parse_descriptor(kind: ParseDiagKind) -> DiagDescriptor {
    match kind {
        ParseDiagKind::Lexical => DiagDescriptor {
            diag_id: "parse.lexical",
            code: "lang::lex::error",
            stage: DiagStage::Parse,
            severity_default: DiagSeverity::Error,
            help_template: "Fix the lexical error near this token.",
        },
        ParseDiagKind::SyntaxError => DiagDescriptor {
            diag_id: "parse.syntax_error",
            code: "lang::parse::syntax_error",
            stage: DiagStage::Parse,
            severity_default: DiagSeverity::Error,
            help_template: "Fix the syntax near this token.",
        },
        ParseDiagKind::UnexpectedToken => DiagDescriptor {
            diag_id: "parse.unexpected_token",
            code: "lang::parse::unexpected_token",
            stage: DiagStage::Parse,
            severity_default: DiagSeverity::Error,
            help_template: "Remove the unexpected token or complete the surrounding construct.",
        },
        ParseDiagKind::ExpectedToken => DiagDescriptor {
            diag_id: "parse.expected_token",
            code: "lang::parse::expected_token",
            stage: DiagStage::Parse,
            severity_default: DiagSeverity::Error,
            help_template: "Insert the expected token and continue the expression/statement.",
        },
        ParseDiagKind::ExpectedStatementBoundary => DiagDescriptor {
            diag_id: "parse.expected_stmt_boundary",
            code: "lang::parse::expected_stmt_boundary",
            stage: DiagStage::Parse,
            severity_default: DiagSeverity::Error,
            help_template: "End the current statement before starting a new one.",
        },
    }
}

pub fn validation_descriptor(kind: ValidationDiagKind) -> DiagDescriptor {
    match kind {
        ValidationDiagKind::AstRule => DiagDescriptor {
            diag_id: "validate.ast_rule",
            code: "lang::validate::ast_rule",
            stage: DiagStage::Validate,
            severity_default: DiagSeverity::Error,
            help_template: "Adjust the declaration so it matches language structural rules.",
        },
        ValidationDiagKind::InvalidLiteral => DiagDescriptor {
            diag_id: "validate.invalid_literal",
            code: "lang::validate::invalid_literal",
            stage: DiagStage::Validate,
            severity_default: DiagSeverity::Error,
            help_template: "Fix this literal to a valid numeric format.",
        },
    }
}

pub fn project_descriptor(kind: ProjectDiagKind) -> DiagDescriptor {
    match kind {
        ProjectDiagKind::LoadError => DiagDescriptor {
            diag_id: "project.load_error",
            code: "lang::project::load_error",
            stage: DiagStage::Project,
            severity_default: DiagSeverity::Error,
            help_template: "Fix the referenced module/project issue and retry.",
        },
        ProjectDiagKind::Parse(kind) => parse_descriptor(kind),
        ProjectDiagKind::Validate(kind) => validation_descriptor(kind),
    }
}

pub fn mir_descriptor(kind: MirDiagKind) -> DiagDescriptor {
    match kind {
        MirDiagKind::MissingTerminator => DiagDescriptor {
            diag_id: "mir.missing_terminator",
            code: "lang::mir::missing_terminator",
            stage: DiagStage::Mir,
            severity_default: DiagSeverity::Error,
            help_template: "Ensure every block ends with a terminator.",
        },
        MirDiagKind::AwaitSuspendableMismatch => DiagDescriptor {
            diag_id: "mir.await_suspendable_mismatch",
            code: "lang::mir::await_suspendable_mismatch",
            stage: DiagStage::Mir,
            severity_default: DiagSeverity::Error,
            help_template: "Mark the function suspendable or remove await.",
        },
        MirDiagKind::PhiOrder => DiagDescriptor {
            diag_id: "mir.phi_order",
            code: "lang::mir::phi_order",
            stage: DiagStage::Mir,
            severity_default: DiagSeverity::Error,
            help_template: "Keep phi statements at the beginning of each block.",
        },
        MirDiagKind::PhiSourceMismatch => DiagDescriptor {
            diag_id: "mir.phi_source_mismatch",
            code: "lang::mir::phi_source_mismatch",
            stage: DiagStage::Mir,
            severity_default: DiagSeverity::Error,
            help_template: "Ensure phi incoming edges match predecessor blocks.",
        },
        MirDiagKind::DefiniteInit => DiagDescriptor {
            diag_id: "mir.definite_init",
            code: "lang::mir::definite_init",
            stage: DiagStage::Mir,
            severity_default: DiagSeverity::Error,
            help_template: "Initialize values before use on every reachable path.",
        },
        MirDiagKind::Internal => DiagDescriptor {
            diag_id: "mir.internal",
            code: "lang::mir::internal",
            stage: DiagStage::Mir,
            severity_default: DiagSeverity::Error,
            help_template: "Inspect MIR construction and validation invariants.",
        },
    }
}
