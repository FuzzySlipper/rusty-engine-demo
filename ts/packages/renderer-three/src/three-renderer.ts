import * as THREE from "three";

import type {
  Geometry,
  Material,
  MeshPayloadDescriptor,
  RenderDiff,
  RenderFrameDiff,
  RenderHandle,
  RenderLayer,
  RenderMetadata,
  RenderNode,
  Transform,
} from "@rusty-engine-demo/render-contracts";

/** Raised when a typed projection operation cannot be applied safely. */
export class RenderApplyError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "RenderApplyError";
  }
}

interface NodeEntry {
  readonly object: THREE.Object3D;
  readonly layer: RenderLayer;
  readonly shape: Geometry["shape"];
  viewMaterial: Material;
  meshMaterialSlots: readonly number[] | null;
}

/**
 * Retained Three.js scene owned entirely by the presentation edge.
 *
 * The operation union is intentionally closed to what Rusty Engine emits today:
 * primitive lifecycle, partial visual updates, and complete inline mesh uploads.
 */
export class ThreeRenderer {
  readonly scene = new THREE.Scene();
  readonly #sceneGroup = new THREE.Group();
  readonly #debugGroup = new THREE.Group();
  readonly #handles = new Map<RenderHandle, NodeEntry>();

  constructor() {
    this.#sceneGroup.name = "scene";
    this.#debugGroup.name = "debug";
    this.scene.add(this.#sceneGroup, this.#debugGroup);
  }

  applyFrame(frame: RenderFrameDiff): void {
    const recursivelyDestroyed = new Set<RenderHandle>();
    for (const operation of frame.ops) {
      if (operation.op === "destroy") {
        if (!this.#handles.has(operation.handle) && recursivelyDestroyed.has(operation.handle)) {
          continue;
        }
        this.#destroy(operation.handle, recursivelyDestroyed);
      } else {
        this.applyDiff(operation);
      }
    }
  }

  applyDiff(operation: RenderDiff): void {
    switch (operation.op) {
      case "create":
        this.#create(operation);
        return;
      case "update":
        this.#update(operation);
        return;
      case "destroy":
        this.#destroy(operation.handle);
        return;
      case "replaceMeshPayload":
        this.#replaceMeshPayload(operation);
        return;
    }
  }

  has(handle: RenderHandle): boolean {
    return this.#handles.has(handle);
  }

  get handleCount(): number {
    return this.#handles.size;
  }

  objectFor(handle: RenderHandle): THREE.Object3D | undefined {
    return this.#handles.get(handle)?.object;
  }

  snapshot(): string {
    const entries = [...this.#handles.entries()].sort(([left], [right]) => left - right);
    if (entries.length === 0) {
      return "(empty scene)\n";
    }
    return `${entries.map(([handle, entry]) => snapshotLine(handle, entry)).join("\n")}\n`;
  }

  dispose(): void {
    const handles = [...this.#handles.entries()]
      .sort((left, right) => objectDepth(right[1].object) - objectDepth(left[1].object))
      .map(([handle]) => handle);
    for (const handle of handles) {
      if (this.#handles.has(handle)) {
        this.#destroy(handle);
      }
    }
    this.scene.clear();
  }

  #layerGroup(layer: RenderLayer): THREE.Group {
    return layer === "debug" ? this.#debugGroup : this.#sceneGroup;
  }

  #create(operation: Extract<RenderDiff, { readonly op: "create" }>): void {
    if (this.#handles.has(operation.handle)) {
      throw new RenderApplyError(`create: handle ${operation.handle} already exists`);
    }
    const parent = operation.parent === null
      ? this.#layerGroup(operation.node.layer)
      : this.#require(operation.parent, "create.parent").object;
    const object = buildObject(operation.node);
    parent.add(object);
    this.#handles.set(operation.handle, {
      object,
      layer: operation.node.layer,
      shape: operation.node.geometry.shape,
      viewMaterial: operation.node.material,
      meshMaterialSlots: null,
    });
  }

  #update(operation: Extract<RenderDiff, { readonly op: "update" }>): void {
    const entry = this.#require(operation.handle, "update");
    if (operation.transform !== null) {
      applyTransform(entry.object, operation.transform);
    }
    if (operation.material !== null) {
      if (entry.meshMaterialSlots === null) {
        applyPrimitiveMaterial(entry, operation.material);
      } else {
        applyUploadedMeshMaterials(entry, operation.material);
      }
      entry.viewMaterial = operation.material;
    }
    if (operation.visible !== null) {
      entry.object.visible = operation.visible;
    }
    if (operation.metadata !== null) {
      applyMetadata(entry.object, operation.metadata);
    }
  }

  #destroy(handle: RenderHandle, recursivelyDestroyed?: Set<RenderHandle>): void {
    const entry = this.#require(handle, "destroy");
    const children = [...this.#handles.entries()]
      .filter(([, candidate]) => candidate.object.parent === entry.object)
      .map(([child]) => child)
      .sort((left, right) => left - right);
    for (const child of children) {
      this.#destroy(child, recursivelyDestroyed);
    }
    entry.object.removeFromParent();
    disposeObject(entry.object);
    this.#handles.delete(handle);
    recursivelyDestroyed?.add(handle);
  }

  #replaceMeshPayload(
    operation: Extract<RenderDiff, { readonly op: "replaceMeshPayload" }>,
  ): void {
    const entry = this.#require(operation.handle, "replaceMeshPayload");
    if (!(entry.object instanceof THREE.Mesh)) {
      throw new RenderApplyError(
        `replaceMeshPayload: handle ${operation.handle} is not a mesh`,
      );
    }

    const geometry = buildMeshGeometry(operation.payload);
    const slots = operation.payload.groups.map((group) => group.materialSlot);
    const materials = slots.map((slot) => uploadedMeshMaterial(slot, entry.viewMaterial));
    const oldGeometry = entry.object.geometry;
    const oldMaterials = meshMaterials(entry.object);

    entry.object.geometry = geometry;
    entry.object.material = materials.length === 1 ? materials[0]! : materials;
    entry.meshMaterialSlots = slots;

    oldGeometry.dispose();
    oldMaterials.forEach((material) => material.dispose());
  }

  #require(handle: RenderHandle, context: string): NodeEntry {
    const entry = this.#handles.get(handle);
    if (entry === undefined) {
      throw new RenderApplyError(`${context}: unknown handle ${handle}`);
    }
    return entry;
  }
}

function buildObject(node: RenderNode): THREE.Object3D {
  const material = buildMaterial(node.geometry.shape, node.material);
  let object: THREE.Object3D;
  switch (node.geometry.shape) {
    case "cube":
      object = new THREE.Mesh(new THREE.BoxGeometry(1, 1, 1), material);
      break;
    case "sphere":
      object = new THREE.Mesh(new THREE.SphereGeometry(0.5, 8, 8), material);
      break;
    case "quad":
      object = new THREE.Mesh(new THREE.PlaneGeometry(1, 1), material);
      break;
    case "point":
      object = new THREE.Points(pointGeometry(), material);
      break;
    case "line":
      object = new THREE.LineSegments(
        lineGeometry(node.geometry.a, node.geometry.b),
        material,
      );
      break;
  }
  applyTransform(object, node.transform);
  object.visible = node.visible;
  applyMetadata(object, node.metadata);
  return object;
}

function buildMaterial(shape: Geometry["shape"], material: Material): THREE.Material {
  const color = new THREE.Color(material.color[0], material.color[1], material.color[2]);
  const opacity = material.color[3];
  const common = { color, opacity, transparent: opacity < 1 };
  switch (shape) {
    case "point":
      return new THREE.PointsMaterial({ ...common, size: 0.1 });
    case "line":
      return new THREE.LineBasicMaterial(common);
    default:
      return new THREE.MeshBasicMaterial({ ...common, wireframe: material.wireframe });
  }
}

function buildMeshGeometry(payload: MeshPayloadDescriptor): THREE.BufferGeometry {
  validateMeshPayload(payload);
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute(
    "position",
    new THREE.BufferAttribute(new Float32Array(payload.source.positions), 3),
  );
  geometry.setAttribute(
    "normal",
    new THREE.BufferAttribute(new Float32Array(payload.source.normals), 3),
  );
  geometry.setIndex(new THREE.BufferAttribute(new Uint32Array(payload.source.indices), 1));
  payload.groups.forEach((group, index) => geometry.addGroup(group.start, group.count, index));
  geometry.boundingBox = new THREE.Box3(
    new THREE.Vector3(...payload.bounds.min),
    new THREE.Vector3(...payload.bounds.max),
  );
  geometry.computeBoundingSphere();
  return geometry;
}

function validateMeshPayload(payload: MeshPayloadDescriptor): void {
  const { vertexCount, indexCount, attributes } = payload.layout;
  requireCount(vertexCount, "layout.vertexCount");
  requireCount(indexCount, "layout.indexCount");
  const position = attributes.filter((attribute) => attribute.name === "position");
  const normal = attributes.filter((attribute) => attribute.name === "normal");
  if (
    position.length !== 1
    || normal.length !== 1
    || position[0]?.components !== 3
    || normal[0]?.components !== 3
  ) {
    throw new RenderApplyError(
      "replaceMeshPayload: layout requires one three-component position and normal stream",
    );
  }
  if (payload.source.positions.length !== vertexCount * 3) {
    throw new RenderApplyError("replaceMeshPayload: position count does not match layout");
  }
  if (payload.source.normals.length !== vertexCount * 3) {
    throw new RenderApplyError("replaceMeshPayload: normal count does not match layout");
  }
  if (payload.source.indices.length !== indexCount) {
    throw new RenderApplyError("replaceMeshPayload: index count does not match layout");
  }
  if (![...payload.source.positions, ...payload.source.normals].every(Number.isFinite)) {
    throw new RenderApplyError("replaceMeshPayload: vertex streams must be finite");
  }
  for (const index of payload.source.indices) {
    if (!Number.isSafeInteger(index) || index < 0 || index >= vertexCount) {
      throw new RenderApplyError(
        `replaceMeshPayload: index ${index} is outside vertex count ${vertexCount}`,
      );
    }
  }
  if (payload.groups.length === 0) {
    throw new RenderApplyError("replaceMeshPayload: at least one material group is required");
  }
  for (const group of payload.groups) {
    if (
      !Number.isSafeInteger(group.materialSlot)
      || group.materialSlot < 0
      || !Number.isSafeInteger(group.start)
      || group.start < 0
      || !Number.isSafeInteger(group.count)
      || group.count <= 0
      || group.start + group.count > indexCount
    ) {
      throw new RenderApplyError("replaceMeshPayload: material group is outside the index stream");
    }
  }
  if (
    ![...payload.bounds.min, ...payload.bounds.max].every(Number.isFinite)
    || payload.bounds.min.some((value, axis) => value > payload.bounds.max[axis]!)
  ) {
    throw new RenderApplyError("replaceMeshPayload: bounds must be finite and ordered");
  }
}

function requireCount(value: number, label: string): void {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new RenderApplyError(`replaceMeshPayload: ${label} must be a non-negative integer`);
  }
}

function uploadedMeshMaterial(slot: number, view: Material): THREE.MeshStandardMaterial {
  const slotColor = new THREE.Color().setHSL((slot * 0.61803398875) % 1, 0.7, 0.5);
  return new THREE.MeshStandardMaterial({
    color: new THREE.Color(
      slotColor.r * view.color[0],
      slotColor.g * view.color[1],
      slotColor.b * view.color[2],
    ),
    opacity: view.color[3],
    transparent: view.color[3] < 1,
    wireframe: view.wireframe,
    roughness: 1,
    metalness: 0,
  });
}

function applyUploadedMeshMaterials(entry: NodeEntry, material: Material): void {
  const mesh = entry.object as THREE.Mesh;
  const previous = meshMaterials(mesh);
  const next = (entry.meshMaterialSlots ?? []).map((slot) =>
    uploadedMeshMaterial(slot, material));
  mesh.material = next.length === 1 ? next[0]! : next;
  previous.forEach((value) => value.dispose());
}

function applyPrimitiveMaterial(entry: NodeEntry, material: Material): void {
  const object = entry.object as THREE.Mesh | THREE.Points | THREE.LineSegments;
  const previous = object.material;
  object.material = buildMaterial(entry.shape, material);
  if (Array.isArray(previous)) {
    previous.forEach((value) => value.dispose());
  } else {
    previous.dispose();
  }
}

function applyTransform(object: THREE.Object3D, transform: Transform): void {
  object.position.set(...transform.translation);
  object.quaternion.set(...transform.rotation);
  object.scale.set(...transform.scale);
}

function applyMetadata(object: THREE.Object3D, metadata: RenderMetadata): void {
  object.name = metadata.label ?? "";
  object.userData = { source: metadata.source, tags: metadata.tags };
}

function meshMaterials(object: THREE.Object3D): THREE.Material[] {
  const material = (object as THREE.Mesh).material;
  return Array.isArray(material) ? material : [material];
}

function pointGeometry(): THREE.BufferGeometry {
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute("position", new THREE.Float32BufferAttribute([0, 0, 0], 3));
  return geometry;
}

function lineGeometry(
  a: readonly [number, number, number],
  b: readonly [number, number, number],
): THREE.BufferGeometry {
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute(
    "position",
    new THREE.Float32BufferAttribute([a[0], a[1], a[2], b[0], b[1], b[2]], 3),
  );
  return geometry;
}

function disposeObject(object: THREE.Object3D): void {
  const disposable = object as Partial<{
    geometry: THREE.BufferGeometry;
    material: THREE.Material | THREE.Material[];
  }>;
  disposable.geometry?.dispose();
  if (Array.isArray(disposable.material)) {
    disposable.material.forEach((material) => material.dispose());
  } else {
    disposable.material?.dispose();
  }
}

function snapshotLine(handle: number, entry: NodeEntry): string {
  const object = entry.object;
  return [
    `handle ${handle}`,
    `layer ${entry.layer}`,
    `shape ${entry.meshMaterialSlots === null ? entry.shape : "mesh"}`,
    `pos ${formatVector(object.position)}`,
    `scale ${formatVector(object.scale)}`,
    `visible ${object.visible}`,
    `label ${JSON.stringify(object.name)}`,
  ].join("  ");
}

function formatVector(vector: THREE.Vector3): string {
  return `${formatNumber(vector.x)},${formatNumber(vector.y)},${formatNumber(vector.z)}`;
}

function formatNumber(value: number): string {
  return String(Number(value.toFixed(4)));
}

function objectDepth(object: THREE.Object3D): number {
  let depth = 0;
  let parent = object.parent;
  while (parent !== null) {
    depth += 1;
    parent = parent.parent;
  }
  return depth;
}
