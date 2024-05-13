use serde::{Deserialize, Serialize};

use hasura_authn_core::Role;
use indexmap::IndexMap;

use open_dds::{commands::CommandName, models::ModelName, types::CustomTypeName};

use crate::stages::{
    boolean_expressions, commands, data_connector_scalar_types, models, relationships, scalar_types,
};
use crate::types::error::Error;
use crate::types::permission::ValueExpression;
use crate::types::subgraph::{Qualified, QualifiedTypeReference};
use open_dds::arguments::ArgumentName;

use crate::helpers::argument::resolve_value_expression_for_argument;

use open_dds::permissions::CommandPermissionsV1;

use std::collections::BTreeMap;

use crate::helpers::typecheck;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CommandWithPermissions {
    pub command: commands::Command,
    pub permissions: BTreeMap<Role, CommandPermission>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CommandPermission {
    pub allow_execution: bool,
    pub argument_presets: BTreeMap<ArgumentName, (QualifiedTypeReference, ValueExpression)>,
}

/// resolve command permissions
pub fn resolve(
    metadata_accessor: &open_dds::accessor::MetadataAccessor,
    commands: &IndexMap<Qualified<CommandName>, commands::Command>,
    object_types: &BTreeMap<Qualified<CustomTypeName>, relationships::ObjectTypeWithRelationships>,
    scalar_types: &BTreeMap<Qualified<CustomTypeName>, scalar_types::ScalarTypeRepresentation>,
    object_boolean_expression_types: &BTreeMap<
        Qualified<CustomTypeName>,
        boolean_expressions::ObjectBooleanExpressionType,
    >,
    models: &IndexMap<Qualified<ModelName>, models::Model>,
    data_connectors: &data_connector_scalar_types::DataConnectorsWithScalars,
) -> Result<IndexMap<Qualified<CommandName>, CommandWithPermissions>, Error> {
    let mut commands_with_permissions: IndexMap<Qualified<CommandName>, CommandWithPermissions> =
        commands
            .iter()
            .map(|(command_name, command)| {
                (
                    command_name.clone(),
                    CommandWithPermissions {
                        command: command.clone(),
                        permissions: BTreeMap::new(),
                    },
                )
            })
            .collect();
    for open_dds::accessor::QualifiedObject {
        subgraph,
        object: command_permissions,
    } in &metadata_accessor.command_permissions
    {
        let command_name = &command_permissions.command_name;
        let qualified_command_name = Qualified::new(subgraph.to_string(), command_name.to_owned());
        let command = commands_with_permissions
            .get_mut(&qualified_command_name)
            .ok_or_else(|| Error::UnknownCommandInCommandPermissions {
                command_name: qualified_command_name.clone(),
            })?;
        if command.permissions.is_empty() {
            command.permissions = resolve_command_permissions(
                &command.command,
                command_permissions,
                object_types,
                scalar_types,
                object_boolean_expression_types,
                models,
                data_connectors,
                subgraph,
            )?;
        } else {
            return Err(Error::DuplicateCommandPermission {
                command_name: qualified_command_name.clone(),
            });
        }
    }
    Ok(commands_with_permissions)
}

pub fn resolve_command_permissions(
    command: &commands::Command,
    permissions: &CommandPermissionsV1,
    object_types: &BTreeMap<Qualified<CustomTypeName>, relationships::ObjectTypeWithRelationships>,
    scalar_types: &BTreeMap<Qualified<CustomTypeName>, scalar_types::ScalarTypeRepresentation>,
    object_boolean_expression_types: &BTreeMap<
        Qualified<CustomTypeName>,
        boolean_expressions::ObjectBooleanExpressionType,
    >,
    models: &IndexMap<Qualified<ModelName>, models::Model>,
    data_connectors: &data_connector_scalar_types::DataConnectorsWithScalars,
    subgraph: &str,
) -> Result<BTreeMap<Role, CommandPermission>, Error> {
    let mut validated_permissions = BTreeMap::new();
    for command_permission in &permissions.permissions {
        let mut argument_presets = BTreeMap::new();

        for argument_preset in &command_permission.argument_presets {
            if argument_presets.contains_key(&argument_preset.argument) {
                return Err(Error::DuplicateCommandArgumentPreset {
                    command_name: command.name.clone(),
                    argument_name: argument_preset.argument.clone(),
                });
            }

            match command.arguments.get(&argument_preset.argument) {
                Some(argument) => {
                    let value_expression = resolve_value_expression_for_argument(
                        &argument_preset.argument,
                        &argument_preset.value,
                        &argument.argument_type,
                        subgraph,
                        object_types,
                        scalar_types,
                        object_boolean_expression_types,
                        models,
                        data_connectors,
                    )?;

                    // additionally typecheck literals
                    // we do this outside the argument resolve so that we can emit a command-specific error
                    // on typechecking failure
                    typecheck::typecheck_value_expression(
                        &argument.argument_type,
                        &argument_preset.value,
                    )
                    .map_err(|type_error| {
                        Error::CommandArgumentPresetTypeError {
                            command_name: command.name.clone(),
                            argument_name: argument_preset.argument.clone(),
                            type_error,
                        }
                    })?;

                    argument_presets.insert(
                        argument_preset.argument.clone(),
                        (argument.argument_type.clone(), value_expression),
                    );
                }
                None => {
                    return Err(Error::CommandArgumentPresetMismatch {
                        command_name: command.name.clone(),
                        argument_name: argument_preset.argument.clone(),
                    });
                }
            }
        }

        let resolved_permission = CommandPermission {
            allow_execution: command_permission.allow_execution,
            argument_presets,
        };
        validated_permissions.insert(command_permission.role.clone(), resolved_permission);
    }
    Ok(validated_permissions)
}
