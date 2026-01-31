//! Diff detection logic for comparing public interfaces.

use std::collections::{HashMap, HashSet};

use interlint_format::{
    ClassSig, EnumSig, Export, FunctionSig, InterfaceSig, Module, Parameter, Property,
    PublicInterface, Signature, TypeAliasSig, TypeInfo, VariableSig,
};

use crate::response::{BreakingChange, BreakingChangeType, Severity};

/// Compare two public interfaces and return a list of breaking changes.
pub fn compare_interfaces(
    previous: &PublicInterface,
    current: &PublicInterface,
    include_additions: bool,
) -> Vec<BreakingChange> {
    let mut changes = Vec::new();

    // Compare entrypoints
    compare_entrypoints(previous, current, &mut changes, include_additions);

    // Compare modules
    compare_modules(previous, current, &mut changes, include_additions);

    // Compare assets
    compare_assets(previous, current, &mut changes, include_additions);

    // Sort by severity (most severe first)
    changes.sort_by(|a, b| b.severity.weight().cmp(&a.severity.weight()));

    changes
}

/// Compare entrypoints between interfaces.
fn compare_entrypoints(
    previous: &PublicInterface,
    current: &PublicInterface,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_entrypoints: HashSet<&str> = previous.entrypoints.iter().map(|e| e.name.as_str()).collect();
    let curr_entrypoints: HashSet<&str> = current.entrypoints.iter().map(|e| e.name.as_str()).collect();

    // Check for removed entrypoints
    for name in prev_entrypoints.difference(&curr_entrypoints) {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::EntrypointRemoved,
                format!("entrypoint:{}", name),
                *name,
                format!("Entrypoint '{}' was removed", name),
            )
            .with_suggestion("Ensure consumers are not importing from this entrypoint"),
        );
    }

    // Check for added entrypoints
    if include_additions {
        for name in curr_entrypoints.difference(&prev_entrypoints) {
            changes.push(BreakingChange::new(
                BreakingChangeType::EntrypointAdded,
                format!("entrypoint:{}", name),
                *name,
                format!("Entrypoint '{}' was added", name),
            ));
        }
    }
}

/// Compare modules between interfaces.
fn compare_modules(
    previous: &PublicInterface,
    current: &PublicInterface,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_modules: HashMap<&str, &Module> =
        previous.modules.iter().map(|m| (m.path.as_str(), m)).collect();
    let curr_modules: HashMap<&str, &Module> =
        current.modules.iter().map(|m| (m.path.as_str(), m)).collect();

    // Check for removed modules
    for (path, _module) in &prev_modules {
        if !curr_modules.contains_key(path) {
            changes.push(
                BreakingChange::new(
                    BreakingChangeType::ModuleRemoved,
                    *path,
                    *path,
                    format!("Module '{}' was removed", path),
                )
                .with_suggestion("Ensure consumers are not importing from this module"),
            );
        }
    }

    // Check for added modules
    if include_additions {
        for (path, _module) in &curr_modules {
            if !prev_modules.contains_key(path) {
                changes.push(BreakingChange::new(
                    BreakingChangeType::ModuleAdded,
                    *path,
                    *path,
                    format!("Module '{}' was added", path),
                ));
            }
        }
    }

    // Compare exports in modules that exist in both
    for (path, prev_module) in &prev_modules {
        if let Some(curr_module) = curr_modules.get(path) {
            compare_exports(prev_module, curr_module, changes, include_additions);
        }
    }
}

/// Compare exports between two modules.
fn compare_exports(
    prev_module: &Module,
    curr_module: &Module,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_exports: HashMap<&str, &Export> =
        prev_module.exports.iter().map(|e| (e.name.as_str(), e)).collect();
    let curr_exports: HashMap<&str, &Export> =
        curr_module.exports.iter().map(|e| (e.name.as_str(), e)).collect();

    // Check for removed exports
    for (name, prev_export) in &prev_exports {
        if !curr_exports.contains_key(name) {
            changes.push(
                BreakingChange::new(
                    BreakingChangeType::ExportRemoved,
                    format!("{}#{}", prev_module.path, name),
                    *name,
                    format!("Export '{}' was removed from '{}'", name, prev_module.path),
                )
                .with_previous(signature_kind_name(&prev_export.signature))
                .with_suggestion(format!(
                    "Remove usages of '{}' or provide a migration path",
                    name
                )),
            );
        }
    }

    // Check for added exports
    if include_additions {
        for (name, _export) in &curr_exports {
            if !prev_exports.contains_key(name) {
                changes.push(BreakingChange::new(
                    BreakingChangeType::ExportAdded,
                    format!("{}#{}", curr_module.path, name),
                    *name,
                    format!("Export '{}' was added to '{}'", name, curr_module.path),
                ));
            }
        }
    }

    // Compare exports that exist in both
    for (name, prev_export) in &prev_exports {
        if let Some(curr_export) = curr_exports.get(name) {
            compare_export_signatures(
                prev_export,
                curr_export,
                &curr_module.path,
                changes,
                include_additions,
            );
        }
    }
}

/// Compare signatures of two exports.
fn compare_export_signatures(
    prev: &Export,
    curr: &Export,
    module_path: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let path = format!("{}#{}", module_path, prev.name);

    // Check if default export status changed
    if prev.is_default != curr.is_default {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::DefaultExportChanged,
                &path,
                &prev.name,
                format!(
                    "Default export status changed for '{}' from {} to {}",
                    prev.name, prev.is_default, curr.is_default
                ),
            )
            .with_previous(prev.is_default.to_string())
            .with_current(curr.is_default.to_string()),
        );
    }

    // Check if export kind changed
    if std::mem::discriminant(&prev.signature) != std::mem::discriminant(&curr.signature) {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::ExportKindChanged,
                &path,
                &prev.name,
                format!(
                    "Export '{}' changed from {} to {}",
                    prev.name,
                    signature_kind_name(&prev.signature),
                    signature_kind_name(&curr.signature)
                ),
            )
            .with_previous(signature_kind_name(&prev.signature))
            .with_current(signature_kind_name(&curr.signature)),
        );
        return; // Don't compare further if kind changed
    }

    // Compare based on signature type
    match (&prev.signature, &curr.signature) {
        (Signature::Function(prev_fn), Signature::Function(curr_fn)) => {
            compare_function_signatures(prev_fn, curr_fn, &path, changes, include_additions);
        }
        (Signature::Class(prev_class), Signature::Class(curr_class)) => {
            compare_class_signatures(prev_class, curr_class, &path, changes, include_additions);
        }
        (Signature::Interface(prev_iface), Signature::Interface(curr_iface)) => {
            compare_interface_signatures(prev_iface, curr_iface, &path, changes, include_additions);
        }
        (Signature::Enum(prev_enum), Signature::Enum(curr_enum)) => {
            compare_enum_signatures(prev_enum, curr_enum, &path, changes, include_additions);
        }
        (Signature::TypeAlias(prev_alias), Signature::TypeAlias(curr_alias)) => {
            compare_type_alias_signatures(prev_alias, curr_alias, &path, changes);
        }
        (Signature::Variable(prev_var), Signature::Variable(curr_var)) => {
            compare_variable_signatures(prev_var, curr_var, &path, changes);
        }
        (Signature::Namespace(prev_ns), Signature::Namespace(curr_ns)) => {
            // For namespaces, recursively compare exports
            compare_namespace_exports(
                &prev_ns.exports,
                &curr_ns.exports,
                &path,
                changes,
                include_additions,
            );
        }
        _ => {}
    }
}

/// Compare function signatures.
fn compare_function_signatures(
    prev: &FunctionSig,
    curr: &FunctionSig,
    path: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    // Check async modifier
    if prev.is_async != curr.is_async {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::AsyncModifierChanged,
                path,
                &prev.name,
                format!(
                    "Function '{}' async modifier changed from {} to {}",
                    prev.name, prev.is_async, curr.is_async
                ),
            )
            .with_previous(format!("async: {}", prev.is_async))
            .with_current(format!("async: {}", curr.is_async)),
        );
    }

    // Check return type
    if !types_equal(&prev.return_type, &curr.return_type) {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::ReturnTypeChanged,
                path,
                &prev.name,
                format!(
                    "Return type of '{}' changed from '{}' to '{}'",
                    prev.name, prev.return_type.repr, curr.return_type.repr
                ),
            )
            .with_previous(&prev.return_type.repr)
            .with_current(&curr.return_type.repr),
        );
    }

    // Compare parameters
    compare_parameters(&prev.parameters, &curr.parameters, path, &prev.name, changes, include_additions);

    // Compare type parameters
    compare_type_parameters(
        &prev.type_parameters,
        &curr.type_parameters,
        path,
        &prev.name,
        changes,
        include_additions,
    );

    // Check visibility
    if prev.visibility != curr.visibility {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::VisibilityChanged,
                path,
                &prev.name,
                format!(
                    "Visibility of '{}' changed from {:?} to {:?}",
                    prev.name, prev.visibility, curr.visibility
                ),
            )
            .with_previous(format!("{:?}", prev.visibility))
            .with_current(format!("{:?}", curr.visibility)),
        );
    }
}

/// Compare parameters between two function signatures.
fn compare_parameters(
    prev: &[Parameter],
    curr: &[Parameter],
    path: &str,
    fn_name: &str,
    changes: &mut Vec<BreakingChange>,
    _include_additions: bool,
) {
    let prev_params: HashMap<&str, &Parameter> =
        prev.iter().map(|p| (p.name.as_str(), p)).collect();
    let curr_params: HashMap<&str, &Parameter> =
        curr.iter().map(|p| (p.name.as_str(), p)).collect();

    // Check for removed parameters
    for (name, _param) in &prev_params {
        if !curr_params.contains_key(name) {
            changes.push(
                BreakingChange::new(
                    BreakingChangeType::ParameterRemoved,
                    path,
                    fn_name,
                    format!("Parameter '{}' was removed from '{}'", name, fn_name),
                )
                .with_previous(format!("parameter: {}", name)),
            );
        }
    }

    // Check for added required parameters
    for (name, param) in &curr_params {
        if !prev_params.contains_key(name) {
            let severity = if param.optional || param.default_value.is_some() {
                Severity::Warning
            } else {
                Severity::Critical
            };
            let change_type = if param.optional || param.default_value.is_some() {
                BreakingChangeType::ParameterAdded
            } else {
                BreakingChangeType::ParameterAdded
            };
            changes.push(
                BreakingChange::new(
                    change_type,
                    path,
                    fn_name,
                    format!(
                        "Parameter '{}' was added to '{}' ({})",
                        name,
                        fn_name,
                        if param.optional { "optional" } else { "required" }
                    ),
                )
                .with_severity(severity)
                .with_current(format!("parameter: {} ({})", name, param.type_info.repr)),
            );
        }
    }

    // Compare parameters that exist in both
    for (name, prev_param) in &prev_params {
        if let Some(curr_param) = curr_params.get(name) {
            // Check type change
            if !types_equal(&prev_param.type_info, &curr_param.type_info) {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::ParameterTypeChanged,
                        path,
                        fn_name,
                        format!(
                            "Type of parameter '{}' in '{}' changed from '{}' to '{}'",
                            name, fn_name, prev_param.type_info.repr, curr_param.type_info.repr
                        ),
                    )
                    .with_previous(&prev_param.type_info.repr)
                    .with_current(&curr_param.type_info.repr),
                );
            }

            // Check optional -> required
            if prev_param.optional && !curr_param.optional && curr_param.default_value.is_none() {
                changes.push(BreakingChange::new(
                    BreakingChangeType::ParameterBecameRequired,
                    path,
                    fn_name,
                    format!(
                        "Parameter '{}' in '{}' changed from optional to required",
                        name, fn_name
                    ),
                ));
            }

            // Check required -> optional (usually safe)
            if !prev_param.optional && curr_param.optional {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::ParameterBecameOptional,
                        path,
                        fn_name,
                        format!(
                            "Parameter '{}' in '{}' changed from required to optional",
                            name, fn_name
                        ),
                    )
                    .with_severity(Severity::Warning),
                );
            }
        }
    }

    // Check parameter order (important for positional arguments)
    let prev_order: Vec<&str> = prev.iter().map(|p| p.name.as_str()).collect();
    let curr_order: Vec<&str> = curr.iter().map(|p| p.name.as_str()).collect();

    // Find common parameters and check their order
    let common_prev: Vec<&str> = prev_order
        .iter()
        .filter(|n| curr_params.contains_key(*n))
        .copied()
        .collect();
    let common_curr: Vec<&str> = curr_order
        .iter()
        .filter(|n| prev_params.contains_key(*n))
        .copied()
        .collect();

    if common_prev != common_curr {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::SignatureChanged,
                path,
                fn_name,
                format!("Parameter order changed in '{}'", fn_name),
            )
            .with_previous(common_prev.join(", "))
            .with_current(common_curr.join(", "))
            .with_severity(Severity::Major),
        );
    }
}

/// Compare type parameters.
fn compare_type_parameters(
    prev: &[interlint_format::TypeParameter],
    curr: &[interlint_format::TypeParameter],
    path: &str,
    name: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_params: HashMap<&str, &interlint_format::TypeParameter> =
        prev.iter().map(|p| (p.name.as_str(), p)).collect();
    let curr_params: HashMap<&str, &interlint_format::TypeParameter> =
        curr.iter().map(|p| (p.name.as_str(), p)).collect();

    // Check for removed type parameters
    for (param_name, _) in &prev_params {
        if !curr_params.contains_key(param_name) {
            changes.push(BreakingChange::new(
                BreakingChangeType::TypeParameterRemoved,
                path,
                name,
                format!("Type parameter '{}' was removed from '{}'", param_name, name),
            ));
        }
    }

    // Check for added type parameters
    if include_additions {
        for (param_name, _) in &curr_params {
            if !prev_params.contains_key(param_name) {
                changes.push(BreakingChange::new(
                    BreakingChangeType::TypeParameterAdded,
                    path,
                    name,
                    format!("Type parameter '{}' was added to '{}'", param_name, name),
                ));
            }
        }
    }

    // Compare constraints
    for (param_name, prev_param) in &prev_params {
        if let Some(curr_param) = curr_params.get(param_name) {
            match (&prev_param.constraint, &curr_param.constraint) {
                (Some(prev_constraint), Some(curr_constraint)) => {
                    if !types_equal(prev_constraint, curr_constraint) {
                        changes.push(
                            BreakingChange::new(
                                BreakingChangeType::TypeParameterConstraintChanged,
                                path,
                                name,
                                format!(
                                    "Constraint for type parameter '{}' in '{}' changed",
                                    param_name, name
                                ),
                            )
                            .with_previous(&prev_constraint.repr)
                            .with_current(&curr_constraint.repr),
                        );
                    }
                }
                (Some(prev_constraint), None) => {
                    changes.push(
                        BreakingChange::new(
                            BreakingChangeType::TypeParameterConstraintChanged,
                            path,
                            name,
                            format!(
                                "Constraint removed from type parameter '{}' in '{}'",
                                param_name, name
                            ),
                        )
                        .with_previous(&prev_constraint.repr)
                        .with_severity(Severity::Warning),
                    );
                }
                (None, Some(curr_constraint)) => {
                    changes.push(
                        BreakingChange::new(
                            BreakingChangeType::TypeParameterConstraintChanged,
                            path,
                            name,
                            format!(
                                "Constraint added to type parameter '{}' in '{}'",
                                param_name, name
                            ),
                        )
                        .with_current(&curr_constraint.repr),
                    );
                }
                (None, None) => {}
            }
        }
    }
}

/// Compare class signatures.
fn compare_class_signatures(
    prev: &ClassSig,
    curr: &ClassSig,
    path: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    // Compare properties
    compare_properties(
        &prev.properties,
        &curr.properties,
        path,
        &prev.name,
        changes,
        include_additions,
    );

    // Compare methods
    compare_methods(
        &prev.methods,
        &curr.methods,
        path,
        &prev.name,
        changes,
        include_additions,
    );

    // Compare type parameters
    compare_type_parameters(
        &prev.type_parameters,
        &curr.type_parameters,
        path,
        &prev.name,
        changes,
        include_additions,
    );

    // Check extends
    match (&prev.extends, &curr.extends) {
        (Some(prev_ext), Some(curr_ext)) => {
            if !types_equal(prev_ext, curr_ext) {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::TypeChanged,
                        path,
                        &prev.name,
                        format!("Base class of '{}' changed", prev.name),
                    )
                    .with_previous(&prev_ext.repr)
                    .with_current(&curr_ext.repr),
                );
            }
        }
        (Some(prev_ext), None) => {
            changes.push(
                BreakingChange::new(
                    BreakingChangeType::TypeChanged,
                    path,
                    &prev.name,
                    format!("Base class '{}' removed from '{}'", prev_ext.repr, prev.name),
                )
                .with_previous(&prev_ext.repr),
            );
        }
        (None, Some(curr_ext)) => {
            if include_additions {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::TypeChanged,
                        path,
                        &prev.name,
                        format!("Base class '{}' added to '{}'", curr_ext.repr, prev.name),
                    )
                    .with_current(&curr_ext.repr)
                    .with_severity(Severity::Warning),
                );
            }
        }
        (None, None) => {}
    }

    // Check abstract modifier
    if prev.is_abstract != curr.is_abstract {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::TypeChanged,
                path,
                &prev.name,
                format!(
                    "Abstract modifier of '{}' changed from {} to {}",
                    prev.name, prev.is_abstract, curr.is_abstract
                ),
            )
            .with_previous(format!("abstract: {}", prev.is_abstract))
            .with_current(format!("abstract: {}", curr.is_abstract)),
        );
    }
}

/// Compare interface signatures.
fn compare_interface_signatures(
    prev: &InterfaceSig,
    curr: &InterfaceSig,
    path: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    // Compare properties
    compare_properties(
        &prev.properties,
        &curr.properties,
        path,
        &prev.name,
        changes,
        include_additions,
    );

    // Compare methods
    compare_methods(
        &prev.methods,
        &curr.methods,
        path,
        &prev.name,
        changes,
        include_additions,
    );

    // Compare type parameters
    compare_type_parameters(
        &prev.type_parameters,
        &curr.type_parameters,
        path,
        &prev.name,
        changes,
        include_additions,
    );

    // Compare extends
    let prev_extends: HashSet<&str> = prev.extends.iter().map(|t| t.repr.as_str()).collect();
    let curr_extends: HashSet<&str> = curr.extends.iter().map(|t| t.repr.as_str()).collect();

    for removed in prev_extends.difference(&curr_extends) {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::TypeChanged,
                path,
                &prev.name,
                format!("Interface '{}' no longer extends '{}'", prev.name, removed),
            )
            .with_previous(*removed),
        );
    }

    if include_additions {
        for added in curr_extends.difference(&prev_extends) {
            changes.push(
                BreakingChange::new(
                    BreakingChangeType::TypeChanged,
                    path,
                    &prev.name,
                    format!("Interface '{}' now extends '{}'", prev.name, added),
                )
                .with_current(*added)
                .with_severity(Severity::Warning),
            );
        }
    }
}

/// Compare properties between two signatures.
fn compare_properties(
    prev: &[Property],
    curr: &[Property],
    path: &str,
    parent_name: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_props: HashMap<&str, &Property> =
        prev.iter().map(|p| (p.name.as_str(), p)).collect();
    let curr_props: HashMap<&str, &Property> =
        curr.iter().map(|p| (p.name.as_str(), p)).collect();

    // Check for removed properties
    for (name, _) in &prev_props {
        if !curr_props.contains_key(name) {
            changes.push(BreakingChange::new(
                BreakingChangeType::PropertyRemoved,
                path,
                parent_name,
                format!("Property '{}' was removed from '{}'", name, parent_name),
            ));
        }
    }

    // Check for added properties
    if include_additions {
        for (name, prop) in &curr_props {
            if !prev_props.contains_key(name) {
                let severity = if prop.optional {
                    Severity::Warning
                } else {
                    Severity::Major
                };
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::PropertyAdded,
                        path,
                        parent_name,
                        format!(
                            "Property '{}' was added to '{}' ({})",
                            name,
                            parent_name,
                            if prop.optional { "optional" } else { "required" }
                        ),
                    )
                    .with_severity(severity),
                );
            }
        }
    }

    // Compare properties that exist in both
    for (name, prev_prop) in &prev_props {
        if let Some(curr_prop) = curr_props.get(name) {
            // Check type change
            if !types_equal(&prev_prop.type_info, &curr_prop.type_info) {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::PropertyTypeChanged,
                        path,
                        parent_name,
                        format!(
                            "Type of property '{}' in '{}' changed from '{}' to '{}'",
                            name, parent_name, prev_prop.type_info.repr, curr_prop.type_info.repr
                        ),
                    )
                    .with_previous(&prev_prop.type_info.repr)
                    .with_current(&curr_prop.type_info.repr),
                );
            }

            // Check optional -> required
            if prev_prop.optional && !curr_prop.optional {
                changes.push(BreakingChange::new(
                    BreakingChangeType::PropertyBecameRequired,
                    path,
                    parent_name,
                    format!(
                        "Property '{}' in '{}' changed from optional to required",
                        name, parent_name
                    ),
                ));
            }

            // Check required -> optional
            if !prev_prop.optional && curr_prop.optional {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::PropertyBecameOptional,
                        path,
                        parent_name,
                        format!(
                            "Property '{}' in '{}' changed from required to optional",
                            name, parent_name
                        ),
                    )
                    .with_severity(Severity::Warning),
                );
            }

            // Check visibility change
            if prev_prop.visibility != curr_prop.visibility {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::VisibilityChanged,
                        path,
                        parent_name,
                        format!(
                            "Visibility of property '{}' in '{}' changed from {:?} to {:?}",
                            name, parent_name, prev_prop.visibility, curr_prop.visibility
                        ),
                    )
                    .with_previous(format!("{:?}", prev_prop.visibility))
                    .with_current(format!("{:?}", curr_prop.visibility)),
                );
            }
        }
    }
}

/// Compare methods between two signatures.
fn compare_methods(
    prev: &[FunctionSig],
    curr: &[FunctionSig],
    path: &str,
    parent_name: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_methods: HashMap<&str, &FunctionSig> =
        prev.iter().map(|m| (m.name.as_str(), m)).collect();
    let curr_methods: HashMap<&str, &FunctionSig> =
        curr.iter().map(|m| (m.name.as_str(), m)).collect();

    // Check for removed methods
    for (name, _) in &prev_methods {
        if !curr_methods.contains_key(name) {
            changes.push(BreakingChange::new(
                BreakingChangeType::MethodRemoved,
                path,
                parent_name,
                format!("Method '{}' was removed from '{}'", name, parent_name),
            ));
        }
    }

    // Check for added methods
    if include_additions {
        for (name, _) in &curr_methods {
            if !prev_methods.contains_key(name) {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::MethodAdded,
                        path,
                        parent_name,
                        format!("Method '{}' was added to '{}'", name, parent_name),
                    )
                    .with_severity(Severity::Warning),
                );
            }
        }
    }

    // Compare methods that exist in both
    for (name, prev_method) in &prev_methods {
        if let Some(curr_method) = curr_methods.get(name) {
            let method_path = format!("{}.{}", path, name);
            compare_function_signatures(
                prev_method,
                curr_method,
                &method_path,
                changes,
                include_additions,
            );
        }
    }
}

/// Compare enum signatures.
fn compare_enum_signatures(
    prev: &EnumSig,
    curr: &EnumSig,
    path: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_members: HashMap<&str, Option<&str>> = prev
        .members
        .iter()
        .map(|m| (m.name.as_str(), m.value.as_deref()))
        .collect();
    let curr_members: HashMap<&str, Option<&str>> = curr
        .members
        .iter()
        .map(|m| (m.name.as_str(), m.value.as_deref()))
        .collect();

    // Check for removed members
    for (name, _) in &prev_members {
        if !curr_members.contains_key(name) {
            changes.push(BreakingChange::new(
                BreakingChangeType::EnumMemberRemoved,
                path,
                &prev.name,
                format!("Enum member '{}' was removed from '{}'", name, prev.name),
            ));
        }
    }

    // Check for added members
    if include_additions {
        for (name, _) in &curr_members {
            if !prev_members.contains_key(name) {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::EnumMemberAdded,
                        path,
                        &prev.name,
                        format!("Enum member '{}' was added to '{}'", name, prev.name),
                    )
                    .with_severity(Severity::Warning),
                );
            }
        }
    }

    // Check for value changes
    for (name, prev_value) in &prev_members {
        if let Some(curr_value) = curr_members.get(name) {
            if prev_value != curr_value {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::EnumMemberValueChanged,
                        path,
                        &prev.name,
                        format!(
                            "Value of enum member '{}' in '{}' changed",
                            name, prev.name
                        ),
                    )
                    .with_previous(prev_value.unwrap_or("(auto)").to_string())
                    .with_current(curr_value.unwrap_or("(auto)").to_string()),
                );
            }
        }
    }

    // Check const modifier
    if prev.is_const != curr.is_const {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::TypeChanged,
                path,
                &prev.name,
                format!(
                    "Const modifier of enum '{}' changed from {} to {}",
                    prev.name, prev.is_const, curr.is_const
                ),
            )
            .with_previous(format!("const: {}", prev.is_const))
            .with_current(format!("const: {}", curr.is_const)),
        );
    }
}

/// Compare type alias signatures.
fn compare_type_alias_signatures(
    prev: &TypeAliasSig,
    curr: &TypeAliasSig,
    path: &str,
    changes: &mut Vec<BreakingChange>,
) {
    if !types_equal(&prev.aliased_type, &curr.aliased_type) {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::TypeChanged,
                path,
                &prev.name,
                format!(
                    "Type alias '{}' changed from '{}' to '{}'",
                    prev.name, prev.aliased_type.repr, curr.aliased_type.repr
                ),
            )
            .with_previous(&prev.aliased_type.repr)
            .with_current(&curr.aliased_type.repr),
        );
    }
}

/// Compare variable signatures.
fn compare_variable_signatures(
    prev: &VariableSig,
    curr: &VariableSig,
    path: &str,
    changes: &mut Vec<BreakingChange>,
) {
    if !types_equal(&prev.type_info, &curr.type_info) {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::TypeChanged,
                path,
                &prev.name,
                format!(
                    "Type of variable '{}' changed from '{}' to '{}'",
                    prev.name, prev.type_info.repr, curr.type_info.repr
                ),
            )
            .with_previous(&prev.type_info.repr)
            .with_current(&curr.type_info.repr),
        );
    }

    // Check const modifier change
    if prev.is_const != curr.is_const {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::TypeChanged,
                path,
                &prev.name,
                format!(
                    "Const modifier of '{}' changed from {} to {}",
                    prev.name, prev.is_const, curr.is_const
                ),
            )
            .with_previous(format!("const: {}", prev.is_const))
            .with_current(format!("const: {}", curr.is_const))
            .with_severity(Severity::Minor),
        );
    }
}

/// Compare namespace exports recursively.
fn compare_namespace_exports(
    prev: &[Signature],
    curr: &[Signature],
    path: &str,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_exports: HashMap<&str, &Signature> =
        prev.iter().map(|s| (s.name(), s)).collect();
    let curr_exports: HashMap<&str, &Signature> =
        curr.iter().map(|s| (s.name(), s)).collect();

    // Check for removed exports
    for (name, _) in &prev_exports {
        if !curr_exports.contains_key(name) {
            changes.push(BreakingChange::new(
                BreakingChangeType::ExportRemoved,
                path,
                *name,
                format!("Export '{}' was removed from namespace", name),
            ));
        }
    }

    // Check for added exports
    if include_additions {
        for (name, _) in &curr_exports {
            if !prev_exports.contains_key(name) {
                changes.push(
                    BreakingChange::new(
                        BreakingChangeType::ExportAdded,
                        path,
                        *name,
                        format!("Export '{}' was added to namespace", name),
                    )
                    .with_severity(Severity::Warning),
                );
            }
        }
    }

    // Compare exports that exist in both
    for (name, prev_sig) in &prev_exports {
        if let Some(curr_sig) = curr_exports.get(name) {
            // Create synthetic exports for comparison
            let prev_export = Export {
                name: name.to_string(),
                original_name: None,
                signature: (*prev_sig).clone(),
                is_default: false,
            };
            let curr_export = Export {
                name: name.to_string(),
                original_name: None,
                signature: (*curr_sig).clone(),
                is_default: false,
            };
            compare_export_signatures(&prev_export, &curr_export, path, changes, include_additions);
        }
    }
}

/// Compare assets between two package interfaces.
fn compare_assets(
    previous: &PublicInterface,
    current: &PublicInterface,
    changes: &mut Vec<BreakingChange>,
    include_additions: bool,
) {
    let prev_assets: HashSet<&str> = previous.assets.iter().map(|a| a.path.as_str()).collect();
    let curr_assets: HashSet<&str> = current.assets.iter().map(|a| a.path.as_str()).collect();

    // Removed assets (breaking change)
    for path in prev_assets.difference(&curr_assets) {
        changes.push(
            BreakingChange::new(
                BreakingChangeType::AssetRemoved,
                format!("asset:{}", path),
                path.to_string(),
                format!("Asset '{}' was removed from the package", path),
            )
            .with_suggestion("Ensure consumers are not referencing this asset".to_string()),
        );
    }

    // Added assets (informational)
    if include_additions {
        for path in curr_assets.difference(&prev_assets) {
            changes.push(BreakingChange::new(
                BreakingChangeType::AssetAdded,
                format!("asset:{}", path),
                path.to_string(),
                format!("Asset '{}' was added to the package", path),
            ));
        }
    }
}

/// Check if two types are equal.
fn types_equal(a: &TypeInfo, b: &TypeInfo) -> bool {
    // Simple comparison based on repr for now
    // Could be enhanced to do structural comparison
    a.repr == b.repr && a.kind == b.kind
}

/// Get a human-readable name for a signature kind.
fn signature_kind_name(sig: &Signature) -> String {
    match sig {
        Signature::Function(_) => "function".to_string(),
        Signature::Class(_) => "class".to_string(),
        Signature::Variable(_) => "variable".to_string(),
        Signature::Interface(_) => "interface".to_string(),
        Signature::Enum(_) => "enum".to_string(),
        Signature::TypeAlias(_) => "type alias".to_string(),
        Signature::Namespace(_) => "namespace".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use interlint_format::{Metadata, TypeInfo, Visibility};

    fn create_test_metadata() -> Metadata {
        Metadata {
            name: "test-package".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            repository: None,
            license: None,
            language: Some("typescript".to_string()),
        }
    }

    #[test]
    fn test_export_removed() {
        let mut prev = PublicInterface::new(create_test_metadata());
        let mut prev_module = Module::new("index.ts");
        prev_module.add_export(Export {
            name: "foo".to_string(),
            original_name: None,
            signature: Signature::Function(FunctionSig {
                name: "foo".to_string(),
                type_parameters: vec![],
                parameters: vec![],
                return_type: TypeInfo::void(),
                is_async: false,
                is_generator: false,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });
        prev.add_module(prev_module);

        let mut curr = PublicInterface::new(create_test_metadata());
        curr.add_module(Module::new("index.ts"));

        let changes = compare_interfaces(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, BreakingChangeType::ExportRemoved);
        assert_eq!(changes[0].severity, Severity::Critical);
    }

    #[test]
    fn test_parameter_added() {
        let mut prev = PublicInterface::new(create_test_metadata());
        let mut prev_module = Module::new("index.ts");
        prev_module.add_export(Export {
            name: "greet".to_string(),
            original_name: None,
            signature: Signature::Function(FunctionSig {
                name: "greet".to_string(),
                type_parameters: vec![],
                parameters: vec![],
                return_type: TypeInfo::primitive("string"),
                is_async: false,
                is_generator: false,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });
        prev.add_module(prev_module);

        let mut curr = PublicInterface::new(create_test_metadata());
        let mut curr_module = Module::new("index.ts");
        curr_module.add_export(Export {
            name: "greet".to_string(),
            original_name: None,
            signature: Signature::Function(FunctionSig {
                name: "greet".to_string(),
                type_parameters: vec![],
                parameters: vec![Parameter {
                    name: "name".to_string(),
                    type_info: TypeInfo::primitive("string"),
                    optional: false,
                    rest: false,
                    default_value: None,
                }],
                return_type: TypeInfo::primitive("string"),
                is_async: false,
                is_generator: false,
                visibility: Visibility::Public,
                doc: None,
            }),
            is_default: false,
        });
        curr.add_module(curr_module);

        let changes = compare_interfaces(&prev, &curr, false);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, BreakingChangeType::ParameterAdded);
        assert_eq!(changes[0].severity, Severity::Critical);
    }

    #[test]
    fn test_no_changes() {
        let prev = PublicInterface::new(create_test_metadata());
        let curr = PublicInterface::new(create_test_metadata());

        let changes = compare_interfaces(&prev, &curr, false);
        assert!(changes.is_empty());
    }
}
