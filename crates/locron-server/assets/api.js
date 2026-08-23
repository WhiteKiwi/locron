// API client: the `locron.api/v1` envelope, double-submit CSRF on cookie
// authentication, and CLI-category error mapping. The session cookie is sent
// automatically by the browser; mutations additionally echo the csrf_token
// cookie in the X-CSRF-Token header.
"use strict";

class ApiError extends Error {
  constructor(code, message, status) {
    super(message);
    this.code = code;
    this.status = status;
  }
}

const Api = (() => {
  function cookie(name) {
    const match = document.cookie.match(
      new RegExp(`(?:^|;\\s*)${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}=([^;]+)`),
    );
    return match ? decodeURIComponent(match[1]) : null;
  }

  async function request(method, path, body) {
    const options = { method, headers: { "Content-Type": "application/json" } };
    if (method !== "GET" && method !== "HEAD") {
      const csrf = cookie("csrf_token");
      if (csrf) options.headers["X-CSRF-Token"] = csrf;
    }
    if (body !== undefined) options.body = JSON.stringify(body);
    const response = await fetch(path, options);
    const text = await response.text();
    let payload = null;
    try {
      payload = JSON.parse(text);
    } catch {
      payload = null;
    }
    if (payload && payload.schema === "locron.api/v1") {
      if (payload.ok) {
        return { data: payload.data, warnings: payload.warnings || [] };
      }
      if (response.status === 401) {
        window.dispatchEvent(new CustomEvent("session-expired"));
      }
      throw new ApiError(payload.error.code, payload.error.message, response.status);
    }
    if (!response.ok) {
      throw new ApiError("http_error", text || `${response.status} ${response.statusText}`, response.status);
    }
    return { data: payload, warnings: [] };
  }

  /// Whether the session cookie is present (the entry gate for the app).
  function hasSession() {
    return cookie("locron_session") !== null;
  }

  function get(path) {
    return request("GET", path);
  }

  function post(path, body) {
    return request("POST", path, body === undefined ? {} : body);
  }

  function put(path, body) {
    return request("PUT", path, body === undefined ? {} : body);
  }

  function del(path) {
    return request("DELETE", path);
  }

  return { get, post, put, del, hasSession, ApiError };
})();
