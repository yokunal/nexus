use super::diagnostic::RustDiagnostic;

/// The BorrowTutor examines compiler error codes and diagnostics to generate
/// deep, actionable advice for the AI agent or developer.
pub struct BorrowTutor;

impl BorrowTutor {
    pub fn advise(diagnostic: &RustDiagnostic) -> Option<String> {
        let code = diagnostic.code.as_ref()?.code.as_str();

        match code {
            "E0382" => Some(Self::advise_e0382(diagnostic)),
            "E0502" => Some(Self::advise_e0502(diagnostic)),
            "E0499" => Some(Self::advise_e0499(diagnostic)),
            "E0597" => Some(Self::advise_e0597(diagnostic)),
            "E0505" => Some(Self::advise_e0505(diagnostic)),
            "E0506" => Some(Self::advise_e0506(diagnostic)),
            "E0515" => Some(Self::advise_e0515(diagnostic)),
            "E0716" => Some(Self::advise_e0716(diagnostic)),
            "E0106" => Some(Self::advise_e0106(diagnostic)),
            "E0621" => Some(Self::advise_e0621(diagnostic)),
            "E0277" => Some(Self::advise_e0277(diagnostic)),
            "E0308" => Some(Self::advise_e0308(diagnostic)),
            "E0433" => Some(Self::advise_e0433(diagnostic)),
            _ => None,
        }
    }

    fn advise_e0382(_diag: &RustDiagnostic) -> String {
        r#"💡 [BorrowTutor: E0382 - Use of Moved Value]
CAUSE: Ownership of this value was transferred (moved) into another function, closure, or variable earlier in this scope.
REMEDIES:
1. Borrow instead of moving: Pass a reference `&val` or `&mut val` if the callee only needs to inspect or mutate it.
2. Clone if necessary: If the callee truly requires full ownership and `T: Clone`, call `.clone()` explicitly before the move.
3. Restructure / Reorder: Perform operations that only read from the value before passing full ownership.
4. If working with `Option<T>` or struct fields, consider `val.as_ref()` or `std::mem::take(&mut field)` to avoid moving out of a borrowed container."#.to_string()
    }

    fn advise_e0502(_diag: &RustDiagnostic) -> String {
        r#"💡 [BorrowTutor: E0502 - Simultaneous Mutable and Immutable Borrow]
CAUSE: Rust's core rule: Aliasing XOR Mutability. You cannot borrow a value as mutable (`&mut`) while an immutable borrow (`&`) is still active.
REMEDIES:
1. Narrow the scope of the immutable borrow: Finish using the immutable reference before taking the mutable reference, or introduce an inner block `{ ... }`.
2. Clone the needed data: If you only need a small string or integer from the immutable borrow, copy/clone that field first so the borrow ends before the mutation.
3. Split borrow across struct fields: If mutating one field while reading another, pass references to individual fields rather than borrowing `&self` or the entire struct."#.to_string()
    }

    fn advise_e0499(_diag: &RustDiagnostic) -> String {
        r#"💡 [BorrowTutor: E0499 - Multiple Mutable Borrows]
CAUSE: You cannot have more than one active `&mut` reference to the same data at the same time in the same scope.
REMEDIES:
1. Finish the first mutation: Ensure the first `&mut` reference drops or is no longer used before creating the second.
2. Split borrows: When borrowing elements from a slice/vector, use `slice.split_at_mut()` instead of indexing multiple times.
3. Interior Mutability: If multiple components need concurrent mutable access, consider `std::cell::RefCell` (single-threaded) or `tokio::sync::Mutex` / `std::sync::RwLock` (concurrent)."#.to_string()
    }

    fn advise_e0597(_diag: &RustDiagnostic) -> String {
        r#"💡 [BorrowTutor: E0597 - Borrowed Value Does Not Live Long Enough]
CAUSE: A reference points to a value that drops or goes out of scope before the reference itself finishes being used.
REMEDIES:
1. Move the referenced variable to an outer scope so its lifetime outlives the borrower.
2. Return an owned value (`String`, `Vec<T>`) instead of a reference (`&str`, `&[T]`) if returning from a function.
3. Use `std::sync::Arc<T>` or `std::rc::Rc<T>` if shared ownership across longer scopes or threads is required."#.to_string()
    }

    fn advise_e0505(_diag: &RustDiagnostic) -> String {
        r#"💡 [BorrowTutor: E0505 - Cannot Move Out Because It Is Borrowed]
CAUSE: A value cannot be moved (which invalidates its memory location) while an active borrow is still pointing to it.
REMEDIES:
1. End the borrow before moving: Don't use the reference after the move point.
2. Pass ownership from the start if the borrow is not strictly necessary.
3. Clone the value if you need an owned copy without invalidating the existing borrow."#.to_string()
    }

    fn advise_e0506(_diag: &RustDiagnostic) -> String {
        r#"💡 [BorrowTutor: E0506 - Cannot Assign Because It Is Borrowed]
CAUSE: You cannot reassign a variable while another reference is actively reading from it.
REMEDIES:
1. Finish reading through the borrow before assigning.
2. Reorder operations so the assignment happens after the borrowed reference's last usage."#
            .to_string()
    }

    fn advise_e0515(_diag: &RustDiagnostic) -> String {
        r#"💡 [BorrowTutor: E0515 - Return Value Referencing Local Variable]
CAUSE: You are returning a reference (`&T`) to a local stack variable created inside the function. When the function returns, the local variable is dropped, leaving a dangling pointer.
REMEDIES:
1. Return the owned value directly (e.g. return `String` instead of `&str`, or `T` instead of `&T`).
2. If taking a borrowed input and returning a reference, bind the output lifetime to the input parameter: `fn foo<'a>(input: &'a str) -> &'a str`."#.to_string()
    }

    fn advise_e0716(_diag: &RustDiagnostic) -> String {
        r#"💡 [BorrowTutor: E0716 - Temporary Value Dropped While Borrowed]
CAUSE: A temporary expression (like `Foo::new().bar()`) created a value that dropped at the end of the statement, but a reference to it was stored.
REMEDIES:
1. Bind the temporary value to a `let` binding in the outer scope first: `let foo = Foo::new(); let ref_to_bar = foo.bar();`."#.to_string()
    }

    fn advise_e0106(_diag: &RustDiagnostic) -> String {
        r#"💡 [BorrowTutor: E0106 - Missing Lifetime Specifier]
CAUSE: Rust needs to know how the lifetime of the returned reference connects to the input parameters.
REMEDIES:
1. If the function accepts references and returns a reference, annotate them with named lifetimes: `fn parse<'a>(input: &'a str) -> &'a str`.
2. Or return an owned value (e.g. `String`, `Vec<T>`)."#.to_string()
    }

    fn advise_e0621(_diag: &RustDiagnostic) -> String {
        r#"💡 [BorrowTutor: E0621 - Explicit Lifetime Required in Parameter]
CAUSE: The compiler cannot infer how long a reference parameter must live to satisfy a struct or return requirement.
REMEDIES:
1. Add matching lifetime parameter to the function signature and reference: `fn process<'a>(item: &'a Data) -> Output<'a>`."#.to_string()
    }

    fn advise_e0277(_diag: &RustDiagnostic) -> String {
        r#"💡 [BorrowTutor: E0277 - Trait Bound Not Satisfied]
CAUSE: A type is used where a specific trait is required (e.g., `Display`, `Clone`, `Send`, `Sync`, `Serialize`).
REMEDIES:
1. Derive the trait on the struct/enum: `#[derive(Clone, Debug, Serialize)]`.
2. Manually implement the trait using `impl Trait for MyType`.
3. If `Send` or `Sync` failed across threads, verify no raw pointers or un-synchronized interior mutability (`Rc`, `RefCell`) are crossing thread boundaries."#.to_string()
    }

    fn advise_e0308(_diag: &RustDiagnostic) -> String {
        r#"💡 [BorrowTutor: E0308 - Mismatched Types]
CAUSE: The expression evaluated to a different type than expected by the signature or variable.
REMEDIES:
1. Inspect the expected vs found types in the compiler span.
2. Common fixes: add or remove `&`, use `.into()`, `.as_ref()`, `Some(...)`, or handle `Result`/`Option` with `?` or `.map()`."#.to_string()
    }

    fn advise_e0433(_diag: &RustDiagnostic) -> String {
        r#"💡 [BorrowTutor: E0433 - Unresolved Crate or Module]
CAUSE: The compiler cannot find the crate, module, or type in the current scope.
REMEDIES:
1. Check `Cargo.toml`: Ensure the dependency is added under `[dependencies]`.
2. Add the proper `use` statement at the top of the file.
3. Check for typos in namespace or module paths."#
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rust_specialist::diagnostic::DiagnosticCode;

    #[test]
    fn test_borrow_tutor_e0382() {
        let diag = RustDiagnostic {
            message: "use of moved value: `s`".to_string(),
            code: Some(DiagnosticCode {
                code: "E0382".to_string(),
                explanation: None,
            }),
            level: "error".to_string(),
            spans: Vec::new(),
            children: Vec::new(),
            rendered: None,
            tutor_advice: None,
        };

        let advice = BorrowTutor::advise(&diag);
        assert!(advice.is_some());
        let text = advice.unwrap();
        assert!(text.contains("E0382"));
        assert!(text.contains("Borrow instead of moving"));
        assert!(text.contains(".clone()"));
    }

    #[test]
    fn test_borrow_tutor_e0502() {
        let diag = RustDiagnostic {
            message: "cannot borrow `x` as mutable because it is also borrowed as immutable"
                .to_string(),
            code: Some(DiagnosticCode {
                code: "E0502".to_string(),
                explanation: None,
            }),
            level: "error".to_string(),
            spans: Vec::new(),
            children: Vec::new(),
            rendered: None,
            tutor_advice: None,
        };

        let advice = BorrowTutor::advise(&diag);
        assert!(advice.is_some());
        let text = advice.unwrap();
        assert!(text.contains("E0502"));
        assert!(text.contains("Aliasing XOR Mutability"));
    }
}
