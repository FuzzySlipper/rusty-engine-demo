/**
 * The complete retained rendering vocabulary used by Rusty Engine today.
 *
 * These values describe presentation work only. They carry no gameplay commands,
 * runtime lifecycle, transport envelope, or replay/certification metadata.
 */

export type EntityId = number & { readonly __brand: "EntityId" };
export const entityId = (raw: number): EntityId => raw as EntityId;

export type TagId = number & { readonly __brand: "TagId" };

export type RenderHandle = number & { readonly __brand: "RenderHandle" };
export const renderHandle = (raw: number): RenderHandle => raw as RenderHandle;

export interface Transform {
  readonly translation: readonly [number, number, number];
  readonly rotation: readonly [number, number, number, number];
  readonly scale: readonly [number, number, number];
}

export type Geometry =
  | { readonly shape: "cube" }
  | { readonly shape: "sphere" }
  | { readonly shape: "quad" }
  | { readonly shape: "point" }
  | {
      readonly shape: "line";
      readonly a: readonly [number, number, number];
      readonly b: readonly [number, number, number];
    };

export interface Material {
  readonly color: readonly [number, number, number, number];
  readonly wireframe: boolean;
}

export type RenderLayer = "scene" | "debug";

export interface RenderMetadata {
  readonly source: EntityId | null;
  readonly tags: readonly TagId[];
  readonly label: string | null;
}

export interface RenderNode {
  readonly geometry: Geometry;
  readonly material: Material;
  readonly transform: Transform;
  readonly visible: boolean;
  readonly layer: RenderLayer;
  readonly metadata: RenderMetadata;
}

export type MeshAttributeName = "position" | "normal";

export interface MeshAttribute {
  readonly name: MeshAttributeName;
  readonly components: number;
  readonly kind: "f32";
}

export interface MeshBufferLayout {
  readonly vertexCount: number;
  readonly indexCount: number;
  readonly indexWidth: "u32";
  readonly attributes: readonly MeshAttribute[];
}

export interface MeshGroupDescriptor {
  readonly materialSlot: number;
  readonly start: number;
  readonly count: number;
}

export interface MeshBoundsDescriptor {
  readonly min: readonly [number, number, number];
  readonly max: readonly [number, number, number];
}

export type MeshProvenance = "voxelChunk" | "staticAsset" | "generated" | "debug";

/** Rusty currently crosses the browser boundary with complete typed arrays. */
export interface InlineMeshPayloadSource {
  readonly kind: "inline";
  readonly positions: readonly number[];
  readonly normals: readonly number[];
  readonly indices: readonly number[];
}

export interface MeshPayloadDescriptor {
  readonly layout: MeshBufferLayout;
  readonly groups: readonly MeshGroupDescriptor[];
  readonly bounds: MeshBoundsDescriptor;
  readonly source: InlineMeshPayloadSource;
  readonly provenance: MeshProvenance;
}

/** The closed operation union emitted by RuntimeProjectionAdapter. */
export type RenderDiff =
  | {
      readonly op: "create";
      readonly handle: RenderHandle;
      readonly parent: RenderHandle | null;
      readonly node: RenderNode;
    }
  | {
      readonly op: "update";
      readonly handle: RenderHandle;
      readonly transform: Transform | null;
      readonly material: Material | null;
      readonly visible: boolean | null;
      readonly metadata: RenderMetadata | null;
    }
  | { readonly op: "destroy"; readonly handle: RenderHandle }
  | {
      readonly op: "replaceMeshPayload";
      readonly handle: RenderHandle;
      readonly payload: MeshPayloadDescriptor;
    };

export interface RenderFrameDiff {
  readonly ops: readonly RenderDiff[];
}
