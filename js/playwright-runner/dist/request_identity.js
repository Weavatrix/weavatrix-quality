/** Privacy-safe request identity. Raw bodies never become evidence. */
import { createHash } from "node:crypto";
export function requestPathIdentity(url) {
    try {
        const parsed = new URL(url);
        return `${parsed.pathname}${parsed.search}` || "/";
    }
    catch {
        return url;
    }
}
export function mediaType(contentType) {
    return (contentType.split(";", 1)[0] ?? "").trim().toLowerCase();
}
export function identifyRequestBytes(_method, path, contentType, body) {
    const identity = { content_type: mediaType(contentType) };
    if (!body || body.byteLength === 0)
        return identity;
    let parsed;
    try {
        parsed = JSON.parse(body.toString("utf8"));
    }
    catch {
        parsed = undefined;
    }
    if (looksLikeGraphql(path, contentType, parsed, body)) {
        const graphql = graphqlIdentity(contentType, body, parsed);
        if (graphql) {
            identity.graphql = graphql;
            return identity;
        }
    }
    identity.body_digest =
        parsed === undefined
            ? sha256Hex(body)
            : sha256Hex(Buffer.from(JSON.stringify(canonicalJson(parsed)), "utf8"));
    return identity;
}
export function networkIdentity(method, path, identity) {
    const parts = [`${method.toUpperCase()} ${path}`];
    if (identity?.content_type)
        parts.push(identity.content_type);
    if (identity?.graphql) {
        parts.push(`gql:${identity.graphql.operation_name || "-"}`);
        parts.push(`q:${identity.graphql.query_digest}`);
        parts.push(`v:${identity.graphql.variables_digest}`);
    }
    else if (identity?.body_digest) {
        parts.push(`body:${identity.body_digest}`);
    }
    return parts.join(" ");
}
export function identityFromProfileEntry(entry) {
    const identity = {
        content_type: entry.request_content_type ?? "",
    };
    if (entry.request_body_digest)
        identity.body_digest = entry.request_body_digest;
    if (entry.graphql_query_digest || entry.graphql_variables_digest) {
        identity.graphql = {
            ...(entry.graphql_operation_name ? { operation_name: entry.graphql_operation_name } : {}),
            query_digest: entry.graphql_query_digest ?? "",
            variables_digest: entry.graphql_variables_digest ?? "",
        };
    }
    return identity;
}
function looksLikeGraphql(path, contentType, json, body) {
    if (contentType === "application/graphql" || contentType.startsWith("application/graphql+")) {
        return true;
    }
    const lower = path.toLowerCase();
    const pathLooksGraphql = lower.includes("/graphql") || lower.endsWith("graphql");
    return ((json !== null &&
        typeof json === "object" &&
        !Array.isArray(json) &&
        typeof json.query === "string") ||
        (pathLooksGraphql && body.byteLength > 0));
}
function graphqlIdentity(contentType, body, json) {
    let query;
    let operationName;
    let variables = {};
    if (contentType === "application/graphql" || contentType.startsWith("application/graphql+")) {
        query = body.toString("utf8").trim();
        if (!query)
            return undefined;
    }
    else {
        if (json === null ||
            typeof json !== "object" ||
            Array.isArray(json) ||
            typeof json.query !== "string" ||
            !json.query.trim()) {
            return undefined;
        }
        const document = json;
        query = document.query;
        if (typeof document.operationName === "string" && document.operationName.trim()) {
            operationName = document.operationName.trim();
        }
        if (document.variables !== undefined)
            variables = document.variables;
    }
    const identity = {
        query_digest: sha256Hex(Buffer.from(normaliseGraphqlQuery(query), "utf8")),
        variables_digest: sha256Hex(Buffer.from(JSON.stringify(canonicalJson(variables)), "utf8")),
    };
    const named = operationName ?? namedGraphqlOperation(query);
    if (named)
        identity.operation_name = named;
    return identity;
}
function namedGraphqlOperation(query) {
    const tokens = query.split(/\s+/).filter(Boolean);
    for (let index = 0; index < tokens.length - 1; index += 1) {
        if (!["query", "mutation", "subscription"].includes(tokens[index] ?? ""))
            continue;
        const name = tokens[index + 1] ?? "";
        if (/^[A-Za-z_][A-Za-z0-9_]*$/.test(name))
            return name;
    }
    return undefined;
}
function normaliseGraphqlQuery(query) {
    return query.split(/\s+/).filter(Boolean).join(" ");
}
function canonicalJson(value) {
    if (Array.isArray(value))
        return value.map(canonicalJson);
    if (value && typeof value === "object") {
        return Object.fromEntries(Object.keys(value)
            .sort()
            .map((key) => [key, canonicalJson(value[key])]));
    }
    return value;
}
function sha256Hex(bytes) {
    return createHash("sha256").update(bytes).digest("hex");
}
