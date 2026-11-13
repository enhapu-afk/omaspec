use super::*;

use super::lay::lay;
use super::paint::draw;
use crate::text::Family;

fn diagram(source: &str) -> super::Diagram {
    from_mermaid(source).unwrap_or_else(|| panic!("refused {source}"))
}

#[test]
fn ports_put_the_server_left_of_the_database_and_the_disks_below() {
    let source = "\
architecture-beta
group api(cloud)[API]
service db(database)[Database] in api
service disk1(disk)[Storage] in api
service disk2(disk)[Storage] in api
service server(server)[Server] in api
db:L -- R:server
disk1:T -- B:server
disk2:T -- B:db
";
    let diagram = diagram(source);
    assert_eq!(diagram.nodes.len(), 4);
    assert_eq!(diagram.groups.len(), 1);
    assert_eq!(diagram.nodes[0].icon, Icon::Database);
    let laid = lay(&diagram, Family::Sans).unwrap();
    let db = &laid.nodes[0].tile;
    let disk1 = &laid.nodes[1].tile;
    let disk2 = &laid.nodes[2].tile;
    let server = &laid.nodes[3].tile;
    assert!(server.right() <= db.x + 1.0, "server left of db");
    assert!(disk1.y >= server.bottom() - 1.0, "disk1 below server");
    assert!(disk2.y >= db.bottom() - 1.0, "disk2 below db");
    let group = laid.groups[0].rect;
    for tile in [db, disk1, disk2, server] {
        assert!(tile.x > group.x && tile.right() < group.right());
        assert!(tile.y > group.y && tile.bottom() < group.bottom());
    }
    let svg = draw(&diagram).unwrap();
    assert!(
        svg.contains("Database")
            && svg.contains("Server")
            && svg.contains("#087EBF")
            && svg.contains("stroke-dasharray")
            && svg.contains("<path"),
        "{svg}"
    );
    assert!(clear(&laid), "a label or a line crosses a box");
}

#[test]
fn a_junction_is_a_dot_and_a_group_edge_leaves_the_frame() {
    let source = "\
architecture-beta
group one(cloud)[One]
group two(cloud)[Two]
service server(server)[Server] in one
service subnet(database)[Subnet] in two
junction hub
server{group}:B --> T:subnet{group}
server:R -- L:hub
";
    let diagram = diagram(source);
    assert!(diagram.nodes[2].junction);
    assert!(diagram.edges[0].from_group && diagram.edges[0].to_group);
    assert!(diagram.edges[0].arrow_end && !diagram.edges[0].arrow_start);
    let laid = lay(&diagram, Family::Sans).unwrap();
    let svg = draw(&diagram).unwrap();
    assert!(!svg.contains(">hub<"), "{svg}");
    assert!(svg.contains("<polygon"), "{svg}");
    let group_edge = &laid.edges[0];
    let start = group_edge.points[0];
    let one = laid.groups[0].rect;
    assert!(
        (start.1 - one.bottom()).abs() < 1.0
            || (start.0 - one.x).abs() < 1.0
            || (start.0 - one.right()).abs() < 1.0
    );
    assert!(clear(&laid), "a label or a line crosses a box");
}

#[test]
fn a_nested_group_stays_inside_its_parent_and_a_dark_page_uses_the_accent() {
    let source = "\
architecture-beta
title Platform
group outer(cloud)[Outer]
group inner(server)[Inner] in outer
service db(database)[DB] in inner
";
    let diagram = diagram(source);
    assert_eq!(diagram.groups[1].parent, Some(0));
    assert_eq!(
        diagram.title.as_ref().map(|title| title.value.as_str()),
        Some("Platform")
    );
    let laid = lay(&diagram, Family::Sans).unwrap();
    let inner = laid.groups[1].rect;
    let outer = laid.groups[0].rect;
    assert!(inner.x > outer.x && inner.right() < outer.right());
    assert!(inner.y > outer.y && inner.bottom() < outer.bottom());
    let mut themed = diagram.clone();
    themed.look.classic = false;
    themed.look.accent = super::Color::hex(0xE68E0D);
    themed.look.background = Some(super::Color::hex(0x121212));
    themed.look.preferred = super::Color::hex(0xBEBEBE);
    themed.look.line = super::Color::hex(0xE68E0D);
    let svg = draw(&themed).unwrap();
    assert!(
        svg.contains("Platform")
            && svg.contains("#E68E0D")
            && svg.contains("#BEBEBE")
            && !svg.contains("#087EBF"),
        "{svg}"
    );
}

#[test]
fn a_nested_group_stays_off_its_sibling() {
    let source = "\
architecture-beta
title Platform
group api(cloud)[API]
group data(disk)[Data] in api
service db(database)[Database] in data
service disk1(disk)[Storage] in api
service disk2(disk)[Archive] in api
service server(server)[Server] in api
service web(internet)[Web]
junction hub
db:L -- R:server
disk1:T -- B:server
disk2:T -- B:db
server:R --> L:hub
web:B --> T:server{group}
";
    let laid = lay(&diagram(source), Family::Sans).unwrap();
    let api = laid.groups[0].rect;
    let data = laid.groups[1].rect;
    let server = laid.nodes[3].tile;
    let web = laid.nodes[4].tile;
    assert!(data.x > api.x && data.right() < api.right());
    assert!(data.y > api.y && data.bottom() < api.bottom());
    assert!(
        !data.intersects(server),
        "the inner frame covers the server"
    );
    assert!(
        !api.intersects(web),
        "the outer frame covers the web service"
    );
    let end = *laid.edges[4].points.last().unwrap();
    assert!(
        (end.1 - api.y).abs() < 1.5,
        "the group edge meets the frame"
    );
    assert!(clear(&laid), "a label or a line crosses a box");
}

#[test]
fn an_icon_pack_a_port_and_a_stray_line_are_named() {
    let icon = mermaid_refusal("architecture-beta\n  service db(logos:aws-aurora)[DB]\n").unwrap();
    assert_eq!(icon, (1, "an icon"));
    let port = mermaid_refusal(
        "architecture-beta\n  service a(server)[A]\n  service b(server)[B]\n  a:L -- L:b\n",
    )
    .unwrap();
    assert_eq!(port.1, "a port");
    let missing = mermaid_refusal(
        "architecture-beta\n  service a(server)[A]\n  service b(server)[B]\n  a --> b\n",
    )
    .unwrap();
    assert_eq!(missing.1, "a port");
    let unknown = mermaid_refusal("architecture-beta\n  direction LR\n").unwrap();
    assert_eq!(unknown.1, "an unknown line");
    let group = mermaid_refusal("architecture-beta\n  service a(server)[A] in missing\n").unwrap();
    assert_eq!(group.1, "a group");
    assert!(from_mermaid("sequenceDiagram\nA->B: hi\n").is_none());
    assert!(mermaid_refusal("sequenceDiagram\nA->B: hi\n").is_none());
    let plain = diagram("architecture-beta\n  service a[Api]\n");
    assert_eq!(plain.nodes[0].icon, Icon::Plain);
    assert_eq!(plain.nodes[0].label, "Api");
    let _ = Side::Left;
}

fn clear(laid: &super::lay::Laid) -> bool {
    let mut boxes = Vec::new();
    for node in &laid.nodes {
        boxes.push(node.tile);
        if let Some(label) = node.label_rect {
            boxes.push(label);
        }
    }
    for group in &laid.groups {
        boxes.push(group.label_rect);
        boxes.push(super::lay::Rect {
            x: group.rect.x + 8.0,
            y: group.rect.y + 8.0,
            w: 22.0,
            h: 22.0,
        });
    }
    for (index, rect) in boxes.iter().enumerate() {
        for other in boxes.iter().skip(index + 1) {
            if rect.intersects(*other) {
                return false;
            }
        }
    }
    for edge in &laid.edges {
        for pair in edge.points.windows(2) {
            if boxes
                .iter()
                .any(|rect| super::route::segment_hits(pair[0], pair[1], *rect))
            {
                return false;
            }
        }
    }
    true
}
