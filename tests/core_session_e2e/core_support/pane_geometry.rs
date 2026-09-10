use crate::support::foundation_harness::FoundationHarness;

#[derive(Clone)]
pub(crate) struct PaneGeometry {
    pub(crate) id: String,
    pub(crate) left: i32,
    pub(crate) width: i32,
}

pub(crate) fn read_pane_geometry(
    harness: &FoundationHarness,
    session_name: &str,
) -> Result<Vec<PaneGeometry>, String> {
    let raw = harness.tmux_capture(&[
        "list-panes",
        "-t",
        &format!("{session_name}:0"),
        "-F",
        "#{pane_id}|#{pane_left}|#{pane_width}",
    ])?;

    let mut panes = Vec::new();
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let mut parts = line.split('|');
        let pane_id = parts.next().unwrap_or_default().to_owned();
        let pane_left = parts
            .next()
            .ok_or_else(|| format!("missing pane_left in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_left in `{line}`: {error}"))?;
        let pane_width = parts
            .next()
            .ok_or_else(|| format!("missing pane_width in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_width in `{line}`: {error}"))?;

        panes.push(PaneGeometry {
            id: pane_id,
            left: pane_left,
            width: pane_width,
        });
    }

    Ok(panes)
}

pub(crate) fn center_pane_from_geometry(geometry: &[PaneGeometry]) -> String {
    geometry
        .iter()
        .max_by_key(|pane| (pane.width, -pane.left))
        .map(|pane| pane.id.clone())
        .unwrap_or_default()
}

pub(crate) fn pane_geometry_by_id<'a>(
    geometry: &'a [PaneGeometry],
    pane_id: &str,
) -> Option<&'a PaneGeometry> {
    geometry.iter().find(|pane| pane.id == pane_id)
}
