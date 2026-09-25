"""Generate small, original FBX fixtures (no third-party assets). Python standard library only."""

from dataclasses import dataclass, field
from pathlib import Path
import struct
import zlib


class Id(int):
    pass


@dataclass
class Array:
    kind: str
    values: list


@dataclass
class Node:
    name: str
    props: list = field(default_factory=list)
    children: list = field(default_factory=list)


def prop(name, kind, *values):
    return Node("P", [name, kind, "", "A", *values])


def settings(z_up=False):
    return Node("GlobalSettings", children=[Node("Version", [1000]), Node("Properties70", children=[
        prop("UpAxis", "int", 2 if z_up else 1), prop("UpAxisSign", "int", 1),
        prop("FrontAxis", "int", 1 if z_up else 2), prop("FrontAxisSign", "int", -1 if z_up else 1),
        prop("CoordAxis", "int", 0), prop("CoordAxisSign", "int", 1),
        prop("UnitScaleFactor", "double", 1.0), prop("OriginalUnitScaleFactor", "double", 1.0),
    ])])


def geometry(points, faces, attributes=False):
    polygon_indices = [index if i < len(face) - 1 else -index - 1
                       for face in faces for i, index in enumerate(face)]
    children = [Node("GeometryVersion", [124]),
                Node("Vertices", [Array("d", [float(v) for point in points for v in point])]),
                Node("PolygonVertexIndex", [Array("i", polygon_indices)])]
    if attributes:
        children += [Node("LayerElementMaterial", [0], [Node("Version", [101]),
            Node("Name", [""]), Node("MappingInformationType", ["ByPolygon"]),
            Node("ReferenceInformationType", ["IndexToDirect"]),
            Node("Materials", [Array("i", [i % 2 for i in range(len(faces))])])]),
            Node("LayerElementColor", [0], [Node("Version", [101]), Node("Name", ["Tint"]),
                Node("MappingInformationType", ["ByPolygonVertex"]), Node("ReferenceInformationType", ["Direct"]),
                Node("Colors", [Array("d", [0.5, 1.0, 1.0, 1.0] * len(polygon_indices))])]),
            Node("LayerElementUV", [0], [Node("Version", [101]), Node("Name", ["UVMap"]),
                Node("MappingInformationType", ["ByPolygonVertex"]), Node("ReferenceInformationType", ["Direct"]),
                Node("UV", [Array("d", [0.0, 0.0] * len(polygon_indices))])]),
            Node("Layer", [0], [Node("Version", [100]), *[
                Node("LayerElement", children=[Node("Type", [kind]), Node("TypedIndex", [0])])
                for kind in ("LayerElementMaterial", "LayerElementColor", "LayerElementUV")]])]
    return Node("Geometry", [Id(1000), "Geometry::TestMesh", "Mesh"], children)


def model(id_, name, translation=(0.0, 0.0, 0.0), scale=(1.0, 1.0, 1.0), geometric=(0.0, 0.0, 0.0), visible=1, kind="Mesh"):
    return Node("Model", [Id(id_), "Model::" + name, kind], [Node("Version", [232]),
        Node("Properties70", children=[prop("Lcl Translation", "Lcl Translation", *translation),
            prop("Lcl Scaling", "Lcl Scaling", *scale), prop("GeometricTranslation", "Vector3D", *geometric),
            prop("Visibility", "Visibility", float(visible))]), Node("Shading", [True]), Node("Culling", ["CullingOff"])])


def material(id_, name, color):
    return Node("Material", [Id(id_), "Material::" + name, ""], [Node("Version", [102]),
        Node("ShadingModel", ["lambert"]), Node("MultiLayer", [0]),
        Node("Properties70", children=[prop("DiffuseColor", "Color", *color), prop("DiffuseFactor", "Number", 1.0)])])


def connection(child, parent, property_=None):
    return Node("C", ["OP" if property_ else "OO", Id(child), Id(parent)] + ([property_] if property_ else []))


def document(objects, connections, z_up=False):
    return [Node("FBXHeaderExtension", children=[Node("FBXHeaderVersion", [1003]), Node("FBXVersion", [7400]),
        Node("Creator", ["Gigantomachia fixture generator"])]), settings(z_up),
        Node("Objects", children=objects), Node("Connections", children=connections)]


def encode_prop(value):
    if isinstance(value, Array):
        raw = struct.pack("<" + value.kind * len(value.values), *value.values)
        payload = zlib.compress(raw)
        return value.kind.encode() + struct.pack("<III", len(value.values), 1, len(payload)) + payload
    if isinstance(value, bool):
        return b"C" + bytes([value])
    if isinstance(value, Id):
        return b"L" + struct.pack("<q", value)
    if isinstance(value, int):
        return b"I" + struct.pack("<i", value)
    if isinstance(value, float):
        return b"D" + struct.pack("<d", value)
    if "::" in value:
        type_, name = value.split("::", 1)
        value = name + "\x00\x01" + type_
    raw = value.encode()
    return b"S" + struct.pack("<I", len(raw)) + raw


def binary(nodes, version):
    header = b"Kaydara FBX Binary  \x00\x1a\x00" + struct.pack("<I", version)
    record = "<QQQB" if version >= 7500 else "<IIIB"
    null = bytes(struct.calcsize(record))

    def encode(node, offset):
        name = node.name.encode()
        properties = b"".join(encode_prop(p) for p in node.props)
        prefix_size = len(null) + len(name) + len(properties)
        children = b""
        for child in node.children:
            children += encode(child, offset + prefix_size + len(children))
        if node.children:
            children += null
        end = offset + prefix_size + len(children)
        return struct.pack(record, end, len(node.props), len(properties), len(name)) + name + properties + children

    data = header
    for node in nodes:
        data += encode(node, len(data))
    return data + null


def ascii_fbx(nodes):
    def value(v):
        if isinstance(v, str):
            return '"' + v + '"'
        return str(int(v)) if isinstance(v, bool) else str(v)

    def encode(node, indent=0):
        pad = "    " * indent
        if len(node.props) == 1 and isinstance(node.props[0], Array):
            array = node.props[0]
            return f"{pad}{node.name}: *{len(array.values)} {{\n{pad}    a: " + ",".join(map(value, array.values)) + f"\n{pad}}}\n"
        result = f"{pad}{node.name}: " + ", ".join(map(value, node.props))
        if node.children:
            result += " {\n" + "".join(encode(child, indent + 1) for child in node.children) + pad + "}"
        return result + "\n"

    return "; FBX 7.4.0 project file\n; Original synthetic fixture, Apache-2.0.\n" + "".join(map(encode, nodes))


def main():
    root = Path(__file__).parent
    points = [(-100, 0, -100), (100, 0, -100), (100, 200, -100), (-100, 200, -100),
              (-100, 0, 100), (100, 0, 100), (100, 200, 100), (-100, 200, 100)]
    faces = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 4, 7, 3], [1, 2, 6, 5], [0, 1, 5, 4], [3, 7, 6, 2]]
    objects = [geometry(points, faces, True), model(100, "RootGroup", (100.0, 0.0, 0.0), kind="Null"),
        model(101, "LeftTower", (-300.0, 0.0, 0.0), geometric=(0.0, 50.0, 0.0)),
        model(102, "MirrorTower", (200.0, 0.0, 0.0), (-1.0, 2.0, 0.5), (0.0, 25.0, 0.0)),
        model(103, "Hidden", (10000.0, 0.0, 0.0), visible=0),
        material(200, "Red", (0.8, 0.15, 0.08)), material(201, "Blue", (0.08, 0.25, 0.8)),
        Node("Texture", [Id(300), "Texture::MissingTexture", ""], [Node("Type", ["TextureVideoClip"]),
            Node("FileName", ["does-not-exist.png"]), Node("RelativeFilename", ["does-not-exist.png"])])]
    links = [connection(100, 0), connection(300, 200, "DiffuseColor")]
    for node in (101, 102, 103):
        links += [connection(node, 100), connection(1000, node), connection(200, node), connection(201, node)]
    nodes = document(objects, links)
    (root / "static_scene_ascii.fbx").write_text(ascii_fbx(nodes))
    (root / "static_scene_binary.fbx").write_bytes(binary(nodes, 7400))
    nodes[0].children[1].props = [7500]
    (root / "static_scene_binary_7500.fbx").write_bytes(binary(nodes, 7500))
    z_up = document([geometry([(0, 0, 0), (100, 0, 0), (0, 0, 100)], [[0, 1, 2]]), model(101, "ZUp")],
        [connection(1000, 101), connection(101, 0)], True)
    (root / "z_up_ascii.fbx").write_text(ascii_fbx(z_up))
    concave = document([geometry([(0, 0, 0), (200, 0, 0), (200, 100, 0), (100, 100, 0), (100, 200, 0), (0, 200, 0)],
        [[0, 1, 2, 3, 4, 5]]), model(101, "Concave")], [connection(1000, 101), connection(101, 0)])
    (root / "concave_ascii.fbx").write_text(ascii_fbx(concave))


if __name__ == "__main__":
    main()
