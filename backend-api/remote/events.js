class RemoteEventHub {
  constructor(maxEventsPerAccount = 128) {
    this.listeners = new Map();
    this.history = new Map();
    this.sequence = 0;
    this.maxEventsPerAccount = maxEventsPerAccount;
  }

  publish(accountKey, type, payload) {
    const event = {
      id: String(++this.sequence),
      type,
      payload,
      emittedAt: new Date().toISOString()
    };
    const history = this.history.get(accountKey) || [];
    history.push(event);
    if (history.length > this.maxEventsPerAccount) {
      history.splice(0, history.length - this.maxEventsPerAccount);
    }
    this.history.set(accountKey, history);
    for (const listener of this.listeners.get(accountKey) || []) listener(event);
    return event;
  }

  subscribe(accountKey, listener, afterEventId = '') {
    const bucket = this.listeners.get(accountKey) || new Set();
    bucket.add(listener);
    this.listeners.set(accountKey, bucket);
    const after = Number.parseInt(String(afterEventId || ''), 10);
    if (Number.isSafeInteger(after)) {
      for (const event of this.history.get(accountKey) || []) {
        if (Number.parseInt(event.id, 10) > after) listener(event);
      }
    }
    return () => {
      bucket.delete(listener);
      if (!bucket.size) this.listeners.delete(accountKey);
    };
  }
}

module.exports = { RemoteEventHub };
