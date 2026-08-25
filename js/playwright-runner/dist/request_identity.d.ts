/** Privacy-safe request identity. Raw bodies never become evidence. */
export type GraphqlIdentity = {
    operation_name?: string;
    query_digest: string;
    variables_digest: string;
};
export type RequestIdentity = {
    content_type: string;
    body_digest?: string;
    graphql?: GraphqlIdentity;
};
export type ProfileIdentityFields = {
    request_content_type?: string;
    request_body_digest?: string;
    graphql_operation_name?: string;
    graphql_query_digest?: string;
    graphql_variables_digest?: string;
};
export declare function requestPathIdentity(url: string): string;
export declare function mediaType(contentType: string): string;
export declare function identifyRequestBytes(_method: string, path: string, contentType: string, body: Buffer | undefined): RequestIdentity;
export declare function networkIdentity(method: string, path: string, identity?: RequestIdentity): string;
export declare function identityFromProfileEntry(entry: ProfileIdentityFields): RequestIdentity;
