mod artifacts;
mod display;
mod integrity;
mod paths;
mod xml_summary;

pub use artifacts::{describe_artifact, valid_pdf, valid_png, KrasOutputConflictError, SavedArtifact};
pub use display::{building_age, parse_flr, print_kras_data_table, split_jibn};
pub use integrity::{is_kras_stage3_output_complete, verify_kras_output};
pub use paths::{
	output_label, output_label_str, property_auto_output_paths, with_building_suffix,
	KrasBuildingOption, KrasOutputTarget, PropertyAutoOutputPaths,
};
pub use xml_summary::{create_oz_structure, parse_kras_xml, KrasXmlData};
