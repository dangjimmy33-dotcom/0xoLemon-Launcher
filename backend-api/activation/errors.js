class ActivationError extends Error {
  constructor(code, message, options = {}) {
    super(message);
    this.name = 'ActivationError';
    this.code = code;
    this.httpStatus = options.httpStatus || 400;
    this.retryAt = options.retryAt || null;
    this.uncertain = Boolean(options.uncertain);
  }
}

function errorResponse(error) {
  const code = error instanceof ActivationError ? error.code : 'INTERNAL_ERROR';
  const message = error instanceof ActivationError ? error.message : 'The activation service could not complete the request.';
  const body = { code, message };
  if (error instanceof ActivationError && error.retryAt) body.retryAt = new Date(error.retryAt).toISOString();
  return body;
}

module.exports = { ActivationError, errorResponse };

