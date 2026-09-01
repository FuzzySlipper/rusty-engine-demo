import {
  rendererResourceContentHash,
  type RustyApplicationContent,
  type RustyApplicationResource,
} from "@rusty-engine/application-host";

const RENDERER_PRELOAD_ARTIFACT = "rusty.product.renderer-preload.v1";

type RendererPreloadResourceKind =
  | "animated-mesh"
  | "audio"
  | "clip-pack"
  | "font"
  | "mesh"
  | "texture";

interface RendererPreloadResourceDescriptor {
  readonly identity: string;
  readonly contentHash: string;
  readonly mediaType: string;
  readonly path: string;
  readonly byteLength: number;
}

interface RendererPreloadDescriptor {
  readonly artifact: typeof RENDERER_PRELOAD_ARTIFACT;
  readonly resources: readonly RendererPreloadResourceDescriptor[];
}

interface RendererResourceFormat {
  readonly mediaType: string;
  readonly extension: string;
}

const RESOURCE_FORMATS: Readonly<Record<RendererPreloadResourceKind, RendererResourceFormat>> = {
  "animated-mesh": { mediaType: "model/gltf-binary", extension: ".glb" },
  audio: { mediaType: "audio/wav", extension: ".wav" },
  "clip-pack": { mediaType: "model/gltf-binary", extension: ".glb" },
  font: { mediaType: "font/woff2", extension: ".woff2" },
  mesh: { mediaType: "application/octet-stream", extension: ".rmesh" },
  texture: { mediaType: "image/png", extension: ".png" },
};

/**
 * Resolves the immutable renderer resources already admitted by ProductDevHost
 * into the public application-host initial-content seam.
 */
export async function loadRendererPreloadInitialContent(): Promise<RustyApplicationContent> {
  const descriptorUrl = new URL("./renderer-preload.json", import.meta.url);
  const descriptorResponse = await fetch(descriptorUrl, {
    cache: "no-store",
    redirect: "error",
  });
  if (!descriptorResponse.ok) {
    throw new Error("Loading Bay renderer preload descriptor is unavailable");
  }

  const descriptor = decodeRendererPreloadDescriptor(await descriptorResponse.json());
  const resources = await Promise.all(
    descriptor.resources.map((resource) => loadRendererResource(resource, descriptorUrl)),
  );

  return Object.freeze({
    frame: Object.freeze({ schemaVersion: 1, ops: Object.freeze([]) }),
    resources: Object.freeze(resources),
  });
}

function decodeRendererPreloadDescriptor(value: unknown): RendererPreloadDescriptor {
  if (!isRecord(value) || value.artifact !== RENDERER_PRELOAD_ARTIFACT || !Array.isArray(value.resources)) {
    throw new Error("Loading Bay renderer preload descriptor is invalid");
  }

  const identities = new Set<string>();
  const paths = new Set<string>();
  const resources = value.resources.map((resource, index) => {
    const byteLength = isRecord(resource) ? resource.byteLength : undefined;
    if (!isRecord(resource)
      || typeof resource.identity !== "string"
      || typeof resource.contentHash !== "string"
      || typeof resource.mediaType !== "string"
      || typeof resource.path !== "string"
      || typeof byteLength !== "number"
      || !Number.isSafeInteger(byteLength)) {
      throw new Error(`Loading Bay renderer preload resource ${String(index)} is invalid`);
    }

    const descriptor = Object.freeze({
      identity: resource.identity,
      contentHash: resource.contentHash,
      mediaType: resource.mediaType,
      path: resource.path,
      byteLength,
    });
    const format = rendererResourceFormat(descriptor);
    if (descriptor.byteLength < 0
      || !isSafeRendererResourcePath(descriptor.path)
      || !descriptor.path.endsWith(format.extension)
      || identities.has(descriptor.identity)
      || paths.has(descriptor.path)) {
      throw new Error(`Loading Bay renderer preload resource ${String(index)} is inadmissible`);
    }
    identities.add(descriptor.identity);
    paths.add(descriptor.path);
    return descriptor;
  });

  return Object.freeze({ artifact: RENDERER_PRELOAD_ARTIFACT, resources: Object.freeze(resources) });
}

function rendererResourceFormat(resource: RendererPreloadResourceDescriptor): RendererResourceFormat {
  const match = /^(animated-mesh|audio|clip-pack|mesh|texture)-resource\/([0-9a-f]{64})$/u.exec(resource.identity)
    ?? /^font\/([0-9a-f]{64})$/u.exec(resource.identity);
  if (match === null) {
    throw new Error(`Loading Bay renderer preload resource ${resource.identity} has an invalid identity`);
  }

  const kind = resource.identity.startsWith("font/") ? "font" : match[1];
  if (!isRendererPreloadResourceKind(kind)
    || resource.contentHash !== `sha256:${match[match.length - 1]}`
    || resource.mediaType !== RESOURCE_FORMATS[kind].mediaType) {
    throw new Error(`Loading Bay renderer preload resource ${resource.identity} has invalid metadata`);
  }
  return RESOURCE_FORMATS[kind];
}

function isRendererPreloadResourceKind(value: string | undefined): value is RendererPreloadResourceKind {
  return value !== undefined && Object.hasOwn(RESOURCE_FORMATS, value);
}

function isSafeRendererResourcePath(path: string): boolean {
  return path.startsWith("content/")
    && !path.includes("\\")
    && !path.includes("%")
    && !path.includes(":")
    && !path.includes("?")
    && !path.includes("#")
    && !/[\u0000-\u001f\u007f\s]/u.test(path)
    && path.split("/").every((part) => part.length > 0 && part !== "." && part !== "..");
}

async function loadRendererResource(
  resource: RendererPreloadResourceDescriptor,
  descriptorUrl: URL,
): Promise<RustyApplicationResource> {
  const resourceUrl = new URL(`./${resource.path}`, descriptorUrl);
  if (resourceUrl.origin !== descriptorUrl.origin || resourceUrl.search !== "" || resourceUrl.hash !== "") {
    throw new Error(`Loading Bay renderer resource ${resource.identity} must remain same-origin`);
  }

  const response = await fetch(resourceUrl, { cache: "no-store", redirect: "error" });
  if (!response.ok || response.headers.get("content-type")?.split(";", 1)[0]?.trim().toLowerCase() !== resource.mediaType) {
    throw new Error(`Loading Bay renderer resource ${resource.identity} is unavailable or has an invalid media type`);
  }

  const data = await response.arrayBuffer();
  const bytes = new Uint8Array(data);
  if (bytes.byteLength !== resource.byteLength) {
    throw new Error(`Loading Bay renderer resource ${resource.identity} length mismatch`);
  }
  const actualHash = await rendererResourceContentHash(data, resource.contentHash);
  if (actualHash !== resource.contentHash) {
    throw new Error(`Loading Bay renderer resource ${resource.identity} hash mismatch`);
  }

  return Object.freeze({
    identity: resource.identity,
    contentHash: resource.contentHash,
    mediaType: resource.mediaType,
    bytes,
  });
}

function isRecord(value: unknown): value is Readonly<Record<string, unknown>> {
  return typeof value === "object" && value !== null;
}
