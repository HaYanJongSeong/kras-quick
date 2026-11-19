use std::path::PathBuf;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{create_oz_structure, output_label, parse_kras_xml, valid_pdf, valid_png,
	with_building_suffix, KrasBuildingOption, KrasOutputTarget, PropertyAutoOutputPaths};

pub fn verify_kras_output(
	paths: &PropertyAutoOutputPaths,
	structure_path: &str,
	target: &KrasOutputTarget,
) -> bool {
	let root_real = match std::fs::canonicalize(&paths.directory) {
		Ok(p) => p,
		Err(_) => return false,
	};
	let struct_real = match std::fs::canonicalize(structure_path) {
		Ok(p) => p,
		Err(_) => return false,
	};
	let rel = match struct_real.strip_prefix(&root_real) {
		Ok(r) => r,
		Err(_) => return false,
	};
	if rel.as_os_str().is_empty() {
		return false;
	}
	let identity = serde_json::to_string(&json!([
		{ "value": target.building.value, "label": target.building.label },
		target.floor_room.as_ref().map(|f| json!({"value": f.value, "label": f.label}))
	])).unwrap_or_default();
	let structure_text = match std::fs::read_to_string(structure_path) {
		Ok(t) => t,
		Err(_) => return false,
	};
	let structure: Value = match serde_json::from_str::<Value>(&structure_text).ok()
		.filter(|value| value.get("_krasIntegrity").is_some()).or_else(|| {
		crate::output_log::entries(structure_path, "output").into_iter().rev().find(|value| {
			let saved = &value["_krasIntegrity"];
			serde_json::to_string(&json!([saved["building"], saved["floorRoom"]])).ok().as_ref() == Some(&identity)
		})
	}) {
		Some(v) => v,
		None => return false,
	};
	let integrity = match structure.get("_krasIntegrity") {
		Some(v) => v.clone(),
		None => return false,
	};
	let version = integrity.get("version").and_then(|v| v.as_i64());
	if !matches!(version, Some(1) | Some(2) | Some(3) | Some(4)) {
		return false;
	}
	let stored_identity = serde_json::to_string(&json!([
		integrity.get("building").cloned().unwrap_or(Value::Null),
		integrity.get("floorRoom").cloned().unwrap_or(Value::Null)
	])).unwrap_or_default();
	if stored_identity != identity {
		return false;
	}
	let artifact_schema = |a: &Value| -> bool {
		a.get("file").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false)
			&& a.get("bytes").and_then(|v| v.as_i64()).map(|n| n > 0).unwrap_or(false)
			&& a.get("sha256").and_then(|v| v.as_str()).map(|s| s.len() == 64).unwrap_or(false)
	};
	let pdf_art = match integrity.get("pdf") { Some(v) => v, None => return false };
	let xml_art = match integrity.get("pageXml") { Some(v) => v, None => return false };
	let json_art = if matches!(version, Some(3) | Some(4)) {
		match integrity.get("dataJson") { Some(value) => Some(value), None => return false }
	} else { None };
	let pngs_art = match integrity.get("pagePngs").and_then(|v| v.as_array()) {
		Some(a) if !a.is_empty() || matches!(version, Some(2) | Some(3) | Some(4)) => a,
		_ => return false,
	};
	for a in std::iter::once(pdf_art).chain(json_art).chain(pngs_art.iter()).chain(std::iter::once(xml_art)) {
		if !artifact_schema(a) { return false; }
	}
	let struct_dir = std::path::Path::new(structure_path).parent().unwrap_or(std::path::Path::new("."));
	let mut files: Vec<PathBuf> = Vec::new();
	for art in std::iter::once(pdf_art).chain(json_art).chain(pngs_art.iter()).chain(std::iter::once(xml_art)) {
		let f = match art.get("file").and_then(|v| v.as_str()) {
			Some(s) => s,
			None => return false,
		};
		if std::path::Path::new(f).is_absolute() {
			return false;
		}
		let resolved = struct_dir.join(f);
		let resolved_real = match std::fs::canonicalize(&resolved) {
			Ok(p) => p,
			Err(_) => return false,
		};
		if !resolved_real.starts_with(&root_real) {
			return false;
		}
		files.push(resolved_real);
	}
	let struct_real_clone = struct_real.clone();
	if files.iter().collect::<std::collections::HashSet<_>>().len() != files.len() {
		return false;
	}
	if files.contains(&struct_real_clone) {
		return false;
	}
	let watched: Vec<PathBuf> = std::iter::once(struct_real.clone()).chain(files.iter().cloned()).collect();
	let fingerprint = |paths: &[PathBuf]| -> Option<String> {
		let parts: Vec<String> = paths.iter().map(|p| {
			let meta = std::fs::metadata(p).ok()?;
			let size = meta.len();
			let mtime = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
			let ctime = meta.created().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
			Some(format!("{}:{}:{}:{}", size, mtime, ctime, p.display()))
		}).collect::<Option<Vec<_>>>()?;
		Some(parts.join("|"))
	};
	let before = match fingerprint(&watched) {
		Some(f) => f,
		None => return false,
	};
	let bytes_list: Vec<Vec<u8>> = match files.iter().map(|f| std::fs::read(f)).collect::<Result<Vec<_>, _>>() {
		Ok(b) => b,
		Err(_) => return false,
	};
	let artifacts = std::iter::once(pdf_art).chain(json_art).chain(pngs_art.iter()).chain(std::iter::once(xml_art));
	for (i, art) in artifacts.enumerate() {
		let data = &bytes_list[i];
		let expected_bytes = art.get("bytes").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
		let expected_sha = art.get("sha256").and_then(|v| v.as_str()).unwrap_or("");
		if data.len() != expected_bytes {
			return false;
		}
		let mut h = Sha256::new();
		h.update(data);
		let actual_sha = format!("{:x}", h.finalize());
		if actual_sha != expected_sha {
			return false;
		}
	}
	let pdf_bytes = &bytes_list[0];
	let xml_bytes = bytes_list.last().unwrap();
	let xml_str = String::from_utf8_lossy(xml_bytes);
	let png_start = if json_art.is_some() { 2 } else { 1 };
	let pngs: Vec<&[u8]> = bytes_list[png_start..bytes_list.len() - 1].iter().map(|v| v.as_slice()).collect();
	if json_art.is_some() {
		let saved = serde_json::from_slice::<Value>(&bytes_list[1]).ok();
		let expected = crate::full_xml_json::convert(&xml_str).ok();
		if saved.is_none() || saved != expected { return false; }
	}
	if !valid_pdf(pdf_bytes) {
		return false;
	}
	for p in &pngs {
		if !valid_png(p) {
			return false;
		}
	}
	let xml_data = parse_kras_xml(&xml_str);
	let address_matches = crate::kras_auto_stages::same_parcel_address(&xml_data.land_loc_nm, &paths.address);
	if version == Some(4) {
		if xml_data.land_loc_nm.is_empty()
			|| integrity["requestedAddress"].as_str() != Some(paths.address.as_str())
			|| integrity["documentAddress"].as_str() != Some(xml_data.land_loc_nm.as_str())
			|| integrity["addressMismatch"].as_bool() != Some(!address_matches) {
			return false;
		}
	} else if !address_matches {
		return false;
	}
	let saved_structure = {
		let mut s = structure.clone();
		if let Some(obj) = s.as_object_mut() {
			obj.remove("_krasIntegrity");
		}
		s
	};
	let expected_structure = create_oz_structure(&xml_data);
	if saved_structure != expected_structure {
		return false;
	}
	let after = match fingerprint(&watched) {
		Some(f) => f,
		None => return false,
	};
	if after != before {
		return false;
	}
	true
}

pub fn is_kras_stage3_output_complete(
	paths: &PropertyAutoOutputPaths,
	building: &KrasBuildingOption,
	floor_room: Option<&KrasBuildingOption>,
) -> bool {
	let target = KrasOutputTarget { building: building.clone(), floor_room: floor_room.cloned() };
	let label = output_label(&target);
	let structure_path = with_building_suffix(&paths.structure_json, &paths.other_directory, &label);
	if verify_kras_output(paths, &structure_path, &target) { return true; }
	let has_current_record = crate::output_log::entries(&structure_path, "output").iter().any(|record| {
		let integrity = &record["_krasIntegrity"];
		integrity["building"] == json!({"value":target.building.value, "label":target.building.label})
			&& integrity["floorRoom"] == target.floor_room.as_ref()
				.map(|floor| json!({"value":floor.value, "label":floor.label})).unwrap_or(Value::Null)
	});
	if has_current_record { return false; }
	{
		let legacy = format!("{}\\oz-structure.json", paths.other_directory);
		let legacy = with_building_suffix(&legacy, &paths.other_directory, &label);
		verify_kras_output(paths, &legacy, &target)
	}
}
