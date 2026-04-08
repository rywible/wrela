fn check_async_actor_usage(
    module: &Module,
    info: &TypeInfo,
    classes: &ClassIndex,
    errors: &mut Vec<TypeError>,
) {
    let (function_by_name, class_method_ids) = build_call_maps(module);
    let func_labels = build_func_labels(module, &class_method_ids);
    let mut direct_await = HashMap::new();
    let mut sync_calls: HashMap<usize, Vec<Idx<Function>>> = HashMap::new();
    let mut cause: HashMap<usize, Option<Idx<Function>>> = HashMap::new();

    for (func_id, func) in module.functions.iter() {
        let fn_info = info.function(func_id);
        let mut has_await = false;
        let mut calls = Vec::new();
        func.visit_analysis_bodies(|body| {
            collect_direct_await_and_sync_calls(
                body,
                fn_info,
                &function_by_name,
                &class_method_ids,
                &mut has_await,
                &mut calls,
            );
        });
        direct_await.insert(func_id.into_raw(), has_await);
        if has_await {
            cause.insert(func_id.into_raw(), None);
        }
        sync_calls.insert(func_id.into_raw(), calls);
    }

    let mut requires_actor = direct_await
        .iter()
        .map(|(id, val)| (*id, *val))
        .collect::<HashMap<_, _>>();
    loop {
        let mut changed = false;
        for (func_id, _) in module.functions.iter() {
            let id = func_id.into_raw();
            if *requires_actor.get(&id).unwrap_or(&false) {
                continue;
            }
            let Some(calls) = sync_calls.get(&id) else {
                continue;
            };
            if let Some(callee) = calls
                .iter()
                .find(|callee| *requires_actor.get(&callee.into_raw()).unwrap_or(&false))
            {
                requires_actor.insert(id, true);
                cause.entry(id).or_insert(Some(*callee));
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut class_requires_actor = HashMap::new();
    let mut class_trace = HashMap::new();
    for (_idx, class) in module.classes.iter() {
        let needs_actor = class
            .methods
            .iter()
            .any(|method_id| *requires_actor.get(&method_id.into_raw()).unwrap_or(&false));
        class_requires_actor.insert(class.name.clone(), needs_actor);
        if needs_actor
            && let Some(method_id) = class
                .methods
                .iter()
                .find(|method_id| *requires_actor.get(&method_id.into_raw()).unwrap_or(&false))
        {
            let trace = build_call_chain(
                *method_id,
                &cause,
                &func_labels,
                "Use `detach` or `Pool.of(...)` to create an actor instance.",
            );
            class_trace.insert(class.name.clone(), trace);
        }
    }

    for (func_id, func) in module.functions.iter() {
        let fn_info = info.function(func_id);
        func.visit_analysis_bodies(|body| {
            check_body_async_usage(
                body,
                fn_info,
                classes,
                &class_method_ids,
                &requires_actor,
                &class_requires_actor,
                &class_trace,
                &cause,
                &func_labels,
                errors,
            );
        });
    }
}

fn build_call_maps(
    module: &Module,
) -> (
    HashMap<SmolStr, Idx<Function>>,
    HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>>,
) {
    let mut method_ids = HashSet::new();
    for (_idx, class) in module.classes.iter() {
        for method_id in &class.methods {
            method_ids.insert(method_id.into_raw());
        }
    }

    let mut function_by_name = HashMap::new();
    for (idx, func) in module.functions.iter() {
        if method_ids.contains(&idx.into_raw()) {
            continue;
        }
        function_by_name.insert(func.name.clone(), idx);
    }

    let mut class_method_ids: HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>> = HashMap::new();
    for (_idx, class) in module.classes.iter() {
        let mut methods = HashMap::new();
        for method_id in &class.methods {
            let method = &module.functions[*method_id];
            methods.insert(method.name.clone(), *method_id);
        }
        class_method_ids.insert(class.name.clone(), methods);
    }

    (function_by_name, class_method_ids)
}

fn build_func_labels(
    module: &Module,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>>,
) -> HashMap<usize, String> {
    let mut labels = HashMap::new();
    for (class_name, methods) in class_method_ids {
        for (method_name, method_id) in methods {
            labels.insert(
                method_id.into_raw(),
                format!("{}.{}", class_name, method_name),
            );
        }
    }
    for (func_id, func) in module.functions.iter() {
        labels
            .entry(func_id.into_raw())
            .or_insert_with(|| func.name.to_string());
    }
    labels
}

fn build_call_chain(
    start: Idx<Function>,
    cause: &HashMap<usize, Option<Idx<Function>>>,
    func_labels: &HashMap<usize, String>,
    hint: &str,
) -> String {
    let mut parts = Vec::new();
    let mut current = Some(start);
    let mut visited = HashSet::new();
    while let Some(func_id) = current {
        if !visited.insert(func_id.into_raw()) {
            break;
        }
        let label = func_labels
            .get(&func_id.into_raw())
            .cloned()
            .unwrap_or_else(|| "<unknown>".to_string());
        parts.push(label);
        current = *cause.get(&func_id.into_raw()).unwrap_or(&None);
    }
    parts.push("await".to_string());
    format!("{hint} Async call chain: {}.", parts.join(" -> "))
}

fn collect_direct_await_and_sync_calls(
    body: &Body,
    fn_info: Option<&FunctionTypeInfo>,
    function_by_name: &HashMap<SmolStr, Idx<Function>>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>>,
    has_await: &mut bool,
    calls: &mut Vec<Idx<Function>>,
) {
    for stmt_id in &body.root_stmts {
        visit_stmt_for_async(
            body,
            *stmt_id,
            fn_info,
            function_by_name,
            class_method_ids,
            has_await,
            calls,
        );
    }
}

fn visit_stmt_for_async(
    body: &Body,
    stmt_id: Idx<Stmt>,
    fn_info: Option<&FunctionTypeInfo>,
    function_by_name: &HashMap<SmolStr, Idx<Function>>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>>,
    has_await: &mut bool,
    calls: &mut Vec<Idx<Function>>,
) {
    match &body.stmts[stmt_id] {
        Stmt::Expr(expr) => visit_expr_for_async(
            body,
            *expr,
            fn_info,
            function_by_name,
            class_method_ids,
            has_await,
            calls,
        ),
        Stmt::Assert { expr, .. } => visit_expr_for_async(
            body,
            *expr,
            fn_info,
            function_by_name,
            class_method_ids,
            has_await,
            calls,
        ),
        Stmt::Require { condition, message } => {
            visit_expr_for_async(
                body,
                *condition,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
            visit_expr_for_async(
                body,
                *message,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
        }
        Stmt::Let { value, .. } | Stmt::Assign { value, .. } | Stmt::Capture { value, .. } => {
            visit_expr_for_async(
                body,
                *value,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            )
        }
        Stmt::Defer { expr } => visit_expr_for_async(
            body,
            *expr,
            fn_info,
            function_by_name,
            class_method_ids,
            has_await,
            calls,
        ),
        Stmt::IgnoreResult { expr } => visit_expr_for_async(
            body,
            *expr,
            fn_info,
            function_by_name,
            class_method_ids,
            has_await,
            calls,
        ),
        Stmt::Optimize { body: stmts, .. } | Stmt::While { body: stmts, .. } => {
            for stmt in stmts {
                visit_stmt_for_async(
                    body,
                    *stmt,
                    fn_info,
                    function_by_name,
                    class_method_ids,
                    has_await,
                    calls,
                );
            }
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            visit_expr_for_async(
                body,
                *condition,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
            for stmt in then_branch {
                visit_stmt_for_async(
                    body,
                    *stmt,
                    fn_info,
                    function_by_name,
                    class_method_ids,
                    has_await,
                    calls,
                );
            }
            if let Some(stmts) = else_branch {
                for stmt in stmts {
                    visit_stmt_for_async(
                        body,
                        *stmt,
                        fn_info,
                        function_by_name,
                        class_method_ids,
                        has_await,
                        calls,
                    );
                }
            }
        }
        Stmt::For {
            iterable,
            body: stmts,
            ..
        } => {
            visit_expr_for_async(
                body,
                *iterable,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
            for stmt in stmts {
                visit_stmt_for_async(
                    body,
                    *stmt,
                    fn_info,
                    function_by_name,
                    class_method_ids,
                    has_await,
                    calls,
                );
            }
        }
        Stmt::Match {
            subject,
            cases,
            otherwise,
        } => {
            visit_expr_for_async(
                body,
                *subject,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
            for case in cases {
                for stmt in &case.body {
                    visit_stmt_for_async(
                        body,
                        *stmt,
                        fn_info,
                        function_by_name,
                        class_method_ids,
                        has_await,
                        calls,
                    );
                }
            }
            if let Some(stmts) = otherwise {
                for stmt in stmts {
                    visit_stmt_for_async(
                        body,
                        *stmt,
                        fn_info,
                        function_by_name,
                        class_method_ids,
                        has_await,
                        calls,
                    );
                }
            }
        }
        Stmt::Return(expr) => {
            if let Some(expr) = expr {
                visit_expr_for_async(
                    body,
                    *expr,
                    fn_info,
                    function_by_name,
                    class_method_ids,
                    has_await,
                    calls,
                );
            }
        }
        Stmt::Use { .. } | Stmt::Break | Stmt::Continue => {}
    }
}

fn visit_expr_for_async(
    body: &Body,
    expr_id: Idx<Expr>,
    fn_info: Option<&FunctionTypeInfo>,
    function_by_name: &HashMap<SmolStr, Idx<Function>>,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>>,
    has_await: &mut bool,
    calls: &mut Vec<Idx<Function>>,
) {
    match &body.exprs[expr_id] {
        Expr::Unary { op, expr, .. } => {
            if matches!(op, UnaryOp::Await | UnaryOp::Fire) {
                *has_await = true;
            }
            visit_expr_for_async(
                body,
                *expr,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
        }
        Expr::TypeApply { callee, .. } => {
            visit_expr_for_async(
                body,
                *callee,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
        }
        Expr::Call { callee, args, .. } => {
            if let Expr::Variable(name) = &body.exprs[*callee] {
                if let Some(target) = function_by_name.get(name) {
                    calls.push(*target);
                }
            } else if let Expr::Member { object, member, .. } = &body.exprs[*callee]
                && let Some(fn_info) = fn_info
                && let Some(obj_ty) = fn_info.expr_type(body, *object)
                && let Type::Named(class_name, _) = obj_ty
                && let Some(methods) = class_method_ids.get(class_name)
                && let Some(method_id) = methods.get(member)
            {
                calls.push(*method_id);
            }
            visit_expr_for_async(
                body,
                *callee,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
            for arg in args {
                match arg {
                    crate::hir::Arg::Positional { value, .. } => visit_expr_for_async(
                        body,
                        *value,
                        fn_info,
                        function_by_name,
                        class_method_ids,
                        has_await,
                        calls,
                    ),
                    crate::hir::Arg::Named { value, .. } => visit_expr_for_async(
                        body,
                        *value,
                        fn_info,
                        function_by_name,
                        class_method_ids,
                        has_await,
                        calls,
                    ),
                }
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            visit_expr_for_async(
                body,
                *lhs,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
            visit_expr_for_async(
                body,
                *rhs,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
        }
        Expr::Crash { expr } => visit_expr_for_async(
            body,
            *expr,
            fn_info,
            function_by_name,
            class_method_ids,
            has_await,
            calls,
        ),
        Expr::Detach { target, .. } => visit_expr_for_async(
            body,
            *target,
            fn_info,
            function_by_name,
            class_method_ids,
            has_await,
            calls,
        ),
        Expr::Member { object, .. } => visit_expr_for_async(
            body,
            *object,
            fn_info,
            function_by_name,
            class_method_ids,
            has_await,
            calls,
        ),
        Expr::Index { object, index, .. } => {
            visit_expr_for_async(
                body,
                *object,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
            visit_expr_for_async(
                body,
                *index,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
        }
        Expr::List(items) => {
            for item in items {
                visit_expr_for_async(
                    body,
                    *item,
                    fn_info,
                    function_by_name,
                    class_method_ids,
                    has_await,
                    calls,
                );
            }
        }
        Expr::Map(items) => {
            for (key, value) in items {
                visit_expr_for_async(
                    body,
                    *key,
                    fn_info,
                    function_by_name,
                    class_method_ids,
                    has_await,
                    calls,
                );
                visit_expr_for_async(
                    body,
                    *value,
                    fn_info,
                    function_by_name,
                    class_method_ids,
                    has_await,
                    calls,
                );
            }
        }
        Expr::StringInterp(parts) => {
            for part in parts {
                if let crate::hir::StringPart::Expr(expr) = part {
                    visit_expr_for_async(
                        body,
                        *expr,
                        fn_info,
                        function_by_name,
                        class_method_ids,
                        has_await,
                        calls,
                    );
                }
            }
        }
        Expr::Closure { body: closure_body, .. } => {
            visit_expr_for_async(
                body,
                *closure_body,
                fn_info,
                function_by_name,
                class_method_ids,
                has_await,
                calls,
            );
        }
        Expr::Literal(_) | Expr::Variable(_) => {}
    }
}

fn check_body_async_usage(
    body: &Body,
    fn_info: Option<&FunctionTypeInfo>,
    classes: &ClassIndex,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>>,
    requires_actor: &HashMap<usize, bool>,
    class_requires_actor: &HashMap<SmolStr, bool>,
    class_trace: &HashMap<SmolStr, String>,
    cause: &HashMap<usize, Option<Idx<Function>>>,
    func_labels: &HashMap<usize, String>,
    errors: &mut Vec<TypeError>,
) {
    for stmt_id in &body.root_stmts {
        check_stmt_async_usage(
            body,
            *stmt_id,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            false,
        );
    }
}

fn check_stmt_async_usage(
    body: &Body,
    stmt_id: Idx<Stmt>,
    fn_info: Option<&FunctionTypeInfo>,
    classes: &ClassIndex,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>>,
    requires_actor: &HashMap<usize, bool>,
    class_requires_actor: &HashMap<SmolStr, bool>,
    class_trace: &HashMap<SmolStr, String>,
    cause: &HashMap<usize, Option<Idx<Function>>>,
    func_labels: &HashMap<usize, String>,
    errors: &mut Vec<TypeError>,
    in_detach: bool,
) {
    match &body.stmts[stmt_id] {
        Stmt::Expr(expr) => check_expr_async_usage(
            body,
            *expr,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            in_detach,
        ),
        Stmt::Assert {
            expr,
            rhs,
            tolerance,
            ..
        } => {
            check_expr_async_usage(
                body,
                *expr,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
            if let Some(rhs) = rhs {
                check_expr_async_usage(
                    body,
                    *rhs,
                    fn_info,
                    classes,
                    class_method_ids,
                    requires_actor,
                    class_requires_actor,
                    class_trace,
                    cause,
                    func_labels,
                    errors,
                    in_detach,
                );
            }
            if let Some(tolerance) = tolerance {
                check_expr_async_usage(
                    body,
                    *tolerance,
                    fn_info,
                    classes,
                    class_method_ids,
                    requires_actor,
                    class_requires_actor,
                    class_trace,
                    cause,
                    func_labels,
                    errors,
                    in_detach,
                );
            }
        }
        Stmt::Require { condition, message } => {
            check_expr_async_usage(
                body,
                *condition,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
            check_expr_async_usage(
                body,
                *message,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
        }
        Stmt::Let { value, .. } | Stmt::Assign { value, .. } | Stmt::Capture { value, .. } => {
            check_expr_async_usage(
                body,
                *value,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
        }
        Stmt::Defer { expr } => check_expr_async_usage(
            body,
            *expr,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            in_detach,
        ),
        Stmt::IgnoreResult { expr } => check_expr_async_usage(
            body,
            *expr,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            in_detach,
        ),
        Stmt::Optimize { body: stmts, .. } | Stmt::While { body: stmts, .. } => {
            for stmt in stmts {
                check_stmt_async_usage(
                    body,
                    *stmt,
                    fn_info,
                    classes,
                    class_method_ids,
                    requires_actor,
                    class_requires_actor,
                    class_trace,
                    cause,
                    func_labels,
                    errors,
                    in_detach,
                );
            }
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            check_expr_async_usage(
                body,
                *condition,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
            for stmt in then_branch {
                check_stmt_async_usage(
                    body,
                    *stmt,
                    fn_info,
                    classes,
                    class_method_ids,
                    requires_actor,
                    class_requires_actor,
                    class_trace,
                    cause,
                    func_labels,
                    errors,
                    in_detach,
                );
            }
            if let Some(stmts) = else_branch {
                for stmt in stmts {
                    check_stmt_async_usage(
                        body,
                        *stmt,
                        fn_info,
                        classes,
                        class_method_ids,
                        requires_actor,
                        class_requires_actor,
                        class_trace,
                        cause,
                        func_labels,
                        errors,
                        in_detach,
                    );
                }
            }
        }
        Stmt::For {
            iterable,
            body: stmts,
            ..
        } => {
            check_expr_async_usage(
                body,
                *iterable,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
            for stmt in stmts {
                check_stmt_async_usage(
                    body,
                    *stmt,
                    fn_info,
                    classes,
                    class_method_ids,
                    requires_actor,
                    class_requires_actor,
                    class_trace,
                    cause,
                    func_labels,
                    errors,
                    in_detach,
                );
            }
        }
        Stmt::Match {
            subject,
            cases,
            otherwise,
        } => {
            check_expr_async_usage(
                body,
                *subject,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
            for case in cases {
                for stmt in &case.body {
                    check_stmt_async_usage(
                        body,
                        *stmt,
                        fn_info,
                        classes,
                        class_method_ids,
                        requires_actor,
                        class_requires_actor,
                        class_trace,
                        cause,
                        func_labels,
                        errors,
                        in_detach,
                    );
                }
            }
            if let Some(stmts) = otherwise {
                for stmt in stmts {
                    check_stmt_async_usage(
                        body,
                        *stmt,
                        fn_info,
                        classes,
                        class_method_ids,
                        requires_actor,
                        class_requires_actor,
                        class_trace,
                        cause,
                        func_labels,
                        errors,
                        in_detach,
                    );
                }
            }
        }
        Stmt::Return(expr) => {
            if let Some(expr) = expr {
                check_expr_async_usage(
                    body,
                    *expr,
                    fn_info,
                    classes,
                    class_method_ids,
                    requires_actor,
                    class_requires_actor,
                    class_trace,
                    cause,
                    func_labels,
                    errors,
                    in_detach,
                );
            }
        }
        Stmt::Use { .. } | Stmt::Break | Stmt::Continue => {}
    }
}

fn check_expr_async_usage(
    body: &Body,
    expr_id: Idx<Expr>,
    fn_info: Option<&FunctionTypeInfo>,
    classes: &ClassIndex,
    class_method_ids: &HashMap<SmolStr, HashMap<SmolStr, Idx<Function>>>,
    requires_actor: &HashMap<usize, bool>,
    class_requires_actor: &HashMap<SmolStr, bool>,
    class_trace: &HashMap<SmolStr, String>,
    cause: &HashMap<usize, Option<Idx<Function>>>,
    func_labels: &HashMap<usize, String>,
    errors: &mut Vec<TypeError>,
    in_detach: bool,
) {
    match &body.exprs[expr_id] {
        Expr::Call { callee, args, .. } => {
            if let Expr::Variable(name) = &body.exprs[*callee] {
                if !in_detach
                    && classes.is_class(name)
                    && class_requires_actor.get(name).copied().unwrap_or(false)
                {
                    errors.push(TypeError::AsyncClassRequiresActor {
                        class: name.clone(),
                        span: span_from_range(body.expr_span(*callee)),
                        help: class_trace.get(name).cloned().unwrap_or_else(|| {
                            "Use `detach` or `Pool.of(...)` to create an actor instance."
                                .to_string()
                        }),
                    });
                }
            } else if let Expr::Member {
                object,
                member,
                member_span,
            } = &body.exprs[*callee]
                && let Some(fn_info) = fn_info
                && let Some(obj_ty) = fn_info.expr_type(body, *object)
                && let Type::Named(class_name, _) = obj_ty
                && let Some(methods) = class_method_ids.get(class_name)
                && let Some(method_id) = methods.get(member)
                && *requires_actor.get(&method_id.into_raw()).unwrap_or(&false)
            {
                let hint = "Call this method on a detached or pooled actor instance.";
                let trace = build_call_chain(*method_id, cause, func_labels, hint);
                errors.push(TypeError::AsyncMethodRequiresActor {
                    class: class_name.clone(),
                    member: member.clone(),
                    span: span_from_range(*member_span),
                    help: trace,
                });
            }
            check_expr_async_usage(
                body,
                *callee,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
            for arg in args {
                match arg {
                    crate::hir::Arg::Positional { value, .. } => check_expr_async_usage(
                        body,
                        *value,
                        fn_info,
                        classes,
                        class_method_ids,
                        requires_actor,
                        class_requires_actor,
                        class_trace,
                        cause,
                        func_labels,
                        errors,
                        in_detach,
                    ),
                    crate::hir::Arg::Named { value, .. } => check_expr_async_usage(
                        body,
                        *value,
                        fn_info,
                        classes,
                        class_method_ids,
                        requires_actor,
                        class_requires_actor,
                        class_trace,
                        cause,
                        func_labels,
                        errors,
                        in_detach,
                    ),
                }
            }
        }
        Expr::TypeApply { callee, .. } => check_expr_async_usage(
            body,
            *callee,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            in_detach,
        ),
        Expr::Unary { expr, .. } => check_expr_async_usage(
            body,
            *expr,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            in_detach,
        ),
        Expr::Binary { lhs, rhs, .. } => {
            check_expr_async_usage(
                body,
                *lhs,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
            check_expr_async_usage(
                body,
                *rhs,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
        }
        Expr::Crash { expr } => check_expr_async_usage(
            body,
            *expr,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            in_detach,
        ),
        Expr::Detach { target, .. } => check_expr_async_usage(
            body,
            *target,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            true,
        ),
        Expr::Member { object, .. } => check_expr_async_usage(
            body,
            *object,
            fn_info,
            classes,
            class_method_ids,
            requires_actor,
            class_requires_actor,
            class_trace,
            cause,
            func_labels,
            errors,
            in_detach,
        ),
        Expr::Index { object, index, .. } => {
            check_expr_async_usage(
                body,
                *object,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
            check_expr_async_usage(
                body,
                *index,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
        }
        Expr::List(items) => {
            for item in items {
                check_expr_async_usage(
                    body,
                    *item,
                    fn_info,
                    classes,
                    class_method_ids,
                    requires_actor,
                    class_requires_actor,
                    class_trace,
                    cause,
                    func_labels,
                    errors,
                    in_detach,
                );
            }
        }
        Expr::Map(items) => {
            for (key, value) in items {
                check_expr_async_usage(
                    body,
                    *key,
                    fn_info,
                    classes,
                    class_method_ids,
                    requires_actor,
                    class_requires_actor,
                    class_trace,
                    cause,
                    func_labels,
                    errors,
                    in_detach,
                );
                check_expr_async_usage(
                    body,
                    *value,
                    fn_info,
                    classes,
                    class_method_ids,
                    requires_actor,
                    class_requires_actor,
                    class_trace,
                    cause,
                    func_labels,
                    errors,
                    in_detach,
                );
            }
        }
        Expr::StringInterp(parts) => {
            for part in parts {
                if let crate::hir::StringPart::Expr(expr) = part {
                    check_expr_async_usage(
                        body,
                        *expr,
                        fn_info,
                        classes,
                        class_method_ids,
                        requires_actor,
                        class_requires_actor,
                        class_trace,
                        cause,
                        func_labels,
                        errors,
                        in_detach,
                    );
                }
            }
        }
        Expr::Closure { body: closure_body, .. } => {
            check_expr_async_usage(
                body,
                *closure_body,
                fn_info,
                classes,
                class_method_ids,
                requires_actor,
                class_requires_actor,
                class_trace,
                cause,
                func_labels,
                errors,
                in_detach,
            );
        }
        Expr::Literal(_) | Expr::Variable(_) => {}
    }
}
