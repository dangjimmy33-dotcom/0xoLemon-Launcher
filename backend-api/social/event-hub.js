const crypto = require('crypto');

class SocialEventHub {
  constructor({ maxEvents = 512, now = () => Date.now() } = {}) {
    this.maxEvents = maxEvents;
    this.now = now;
    this.events = [];
    this.subscribers = new Map();
  }

  publish(tenantId, audience, type, payload) {
    const event = {
      id: `${this.now()}-${crypto.randomBytes(6).toString('hex')}`,
      tenantId,
      audience: new Set(audience || []),
      type,
      payload,
      createdAt: new Date(this.now()).toISOString()
    };
    this.events.push(event);
    if (this.events.length > this.maxEvents) this.events.splice(0, this.events.length - this.maxEvents);
    for (const subscriber of this.subscribers.values()) {
      if (subscriber.tenantId === tenantId && this.visibleTo(event, subscriber.userId)) {
        subscriber.send(event);
      }
    }
    return event;
  }

  visibleTo(event, userId) {
    return event.audience.size === 0 || event.audience.has(userId);
  }

  canReplay(tenantId, lastEventId) {
    return !lastEventId || this.events.some((event) => event.tenantId === tenantId && event.id === lastEventId);
  }

  subscribe({ tenantId, userId, lastEventId, send }) {
    const id = crypto.randomUUID();
    if (lastEventId) {
      const offset = this.events.findIndex((event) => event.id === lastEventId);
      if (offset >= 0) {
        for (const event of this.events.slice(offset + 1)) {
          if (event.tenantId === tenantId && this.visibleTo(event, userId)) send(event);
        }
      }
    }
    this.subscribers.set(id, { tenantId, userId, send });
    return () => this.subscribers.delete(id);
  }
}

module.exports = { SocialEventHub };
