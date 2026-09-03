class SocialError extends Error {
  constructor(code, message, httpStatus = 400, details = undefined) {
    super(message);
    this.name = 'SocialError';
    this.code = code;
    this.httpStatus = httpStatus;
    this.details = details;
  }
}

function sendSocialError(res, error) {
  const known = error instanceof SocialError || (error && error.name === 'ActivationError');
  if (!known) console.error('[social] internal failure:', error && error.message);
  const status = known ? Number(error.httpStatus || 400) : 500;
  res.status(status).json({
    code: known ? error.code : 'INTERNAL_ERROR',
    message: known ? error.message : 'Social service could not complete this request.',
    ...(known && error.details ? { details: error.details } : {})
  });
}

module.exports = { SocialError, sendSocialError };
