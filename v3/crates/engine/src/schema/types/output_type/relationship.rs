use std::collections::BTreeMap;

use open_dds::{
    commands::{CommandName, FunctionName},
    models::ModelName,
    relationships::{RelationshipName, RelationshipType},
    types::CustomTypeName,
};

use serde::{Deserialize, Serialize};

use crate::{
    metadata::resolved::{
        self, deserialize_qualified_btreemap, serialize_qualified_btreemap, Qualified,
        QualifiedTypeReference,
    },
    schema::{
        self,
        types::{CommandSourceDetail, TypeKind},
    },
};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ModelRelationshipAnnotation {
    pub source_type: Qualified<CustomTypeName>,
    pub relationship_name: RelationshipName,
    pub model_name: Qualified<ModelName>,
    pub target_source: Option<resolved::ModelTargetSource>,
    pub target_type: Qualified<CustomTypeName>,
    pub relationship_type: RelationshipType,
    pub mappings: Vec<resolved::RelationshipModelMapping>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct FilterRelationshipAnnotation {
    pub relationship_name: RelationshipName,
    pub relationship_type: RelationshipType,
    pub source_type: Qualified<CustomTypeName>,
    pub source_data_connector: resolved::DataConnectorLink,
    #[serde(
        serialize_with = "serialize_qualified_btreemap",
        deserialize_with = "deserialize_qualified_btreemap"
    )]
    pub source_type_mappings: BTreeMap<Qualified<CustomTypeName>, resolved::TypeMapping>,
    pub target_source: resolved::ModelTargetSource,
    pub target_type: Qualified<CustomTypeName>,
    pub target_model_name: Qualified<ModelName>,
    pub mappings: Vec<resolved::RelationshipModelMapping>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct OrderByRelationshipAnnotation {
    pub relationship_name: RelationshipName,
    pub relationship_type: RelationshipType,
    pub source_type: Qualified<CustomTypeName>,
    pub source_data_connector: resolved::DataConnectorLink,
    #[serde(
        serialize_with = "serialize_qualified_btreemap",
        deserialize_with = "deserialize_qualified_btreemap"
    )]
    pub source_type_mappings: BTreeMap<Qualified<CustomTypeName>, resolved::TypeMapping>,
    pub target_source: resolved::ModelTargetSource,
    pub target_type: Qualified<CustomTypeName>,
    pub target_model_name: Qualified<ModelName>,
    pub mappings: Vec<resolved::RelationshipModelMapping>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CommandRelationshipAnnotation {
    pub source_type: Qualified<CustomTypeName>,
    pub relationship_name: RelationshipName,
    pub command_name: Qualified<CommandName>,
    pub target_source: Option<CommandTargetSource>,
    pub target_type: QualifiedTypeReference,
    pub target_base_type_kind: TypeKind,
    pub mappings: Vec<resolved::RelationshipCommandMapping>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CommandTargetSource {
    pub(crate) details: CommandSourceDetail,
    pub(crate) function_name: FunctionName,
    pub(crate) capabilities: resolved::RelationshipCapabilities,
}

impl CommandTargetSource {
    pub fn new(
        command: &resolved::CommandWithPermissions,
        relationship: &resolved::Relationship,
    ) -> Result<Option<Self>, schema::Error> {
        command
            .command
            .source
            .as_ref()
            .map(|command_source| {
                Ok(Self {
                    details: CommandSourceDetail {
                        data_connector: command_source.data_connector.clone(),
                        type_mappings: command_source.type_mappings.clone(),
                        argument_mappings: command_source.argument_mappings.clone(),
                    },
                    function_name: match &command_source.source {
                        schema::types::output_type::DataConnectorCommand::Function(
                            function_name,
                        ) => function_name.clone(),
                        schema::types::output_type::DataConnectorCommand::Procedure(_) => Err(
                            schema::Error::RelationshipsToProcedureBasedCommandsAreNotSupported,
                        )?,
                    },
                    capabilities: relationship
                        .target_capabilities
                        .as_ref()
                        .ok_or_else(|| schema::Error::InternalMissingRelationshipCapabilities {
                            type_name: relationship.source.clone(),
                            relationship: relationship.name.clone(),
                        })?
                        .clone(),
                })
            })
            .transpose()
    }
}
