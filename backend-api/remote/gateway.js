const { randomToken } = require('./security');

class DeviceGateway {
  constructor(eventHub, now = () => Date.now()) {
    this.eventHub = eventHub;
    this.now = now;
    this.connections = new Map();
    this.pendingAcks = new Map();
  }

  key(accountKey, deviceId) {
    return `${accountKey}:${deviceId}`;
  }

  attach(accountKey, deviceId, socket) {
    const key = this.key(accountKey, deviceId);
    const previous = this.connections.get(key);
    if (previous && previous.socket !== socket) previous.socket.close(4001, 'Replaced by a newer connection');
    const connection = { accountKey, deviceId, socket, connectedAt: this.now(), lastSeen: this.now() };
    this.connections.set(key, connection);
    this.eventHub.publish(accountKey, 'device.online', { deviceId, online: true });
    return connection;
  }

  detach(connection) {
    const key = this.key(connection.accountKey, connection.deviceId);
    if (this.connections.get(key)?.socket !== connection.socket) return;
    this.connections.delete(key);
    this.eventHub.publish(connection.accountKey, 'device.offline', { deviceId: connection.deviceId, online: false });
  }

  touch(connection) {
    connection.lastSeen = this.now();
  }

  isOnline(accountKey, deviceId) {
    const connection = this.connections.get(this.key(accountKey, deviceId));
    return Boolean(connection && connection.socket.readyState === 1 && this.now() - connection.lastSeen < 60_000);
  }

  onlineDeviceIds(accountKey) {
    const ids = [];
    for (const connection of this.connections.values()) {
      if (connection.accountKey === accountKey && this.isOnline(accountKey, connection.deviceId)) ids.push(connection.deviceId);
    }
    return ids;
  }

  acknowledge(accountKey, deviceId, jobId, dispatchNonce, accepted, detail = '') {
    const pending = this.pendingAcks.get(jobId);
    if (!pending) return false;
    if (
      pending.accountKey !== accountKey ||
      pending.deviceId !== deviceId ||
      pending.dispatchNonce !== dispatchNonce
    ) {
      return false;
    }
    this.pendingAcks.delete(jobId);
    clearTimeout(pending.timer);
    pending.resolve({ accepted: Boolean(accepted), detail: String(detail || '') });
    return true;
  }

  dispatch(accountKey, deviceId, job, timeoutMs = 10_000) {
    const connection = this.connections.get(this.key(accountKey, deviceId));
    if (!connection || !this.isOnline(accountKey, deviceId)) {
      return Promise.resolve({ accepted: false, detail: 'DEVICE_UNAVAILABLE' });
    }
    const dispatchNonce = randomToken(16);
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.pendingAcks.delete(job.id);
        resolve({ accepted: false, detail: 'DEVICE_ACK_TIMEOUT' });
      }, timeoutMs);
      this.pendingAcks.set(job.id, {
        accountKey,
        deviceId,
        resolve,
        timer,
        dispatchNonce
      });
      try {
        connection.socket.send(JSON.stringify({ type: 'remoteJob.dispatch', dispatchNonce, job }));
      } catch {
        clearTimeout(timer);
        this.pendingAcks.delete(job.id);
        resolve({ accepted: false, detail: 'DEVICE_UNAVAILABLE' });
      }
    });
  }

  send(accountKey, deviceId, message) {
    const connection = this.connections.get(this.key(accountKey, deviceId));
    if (!connection || !this.isOnline(accountKey, deviceId)) return false;
    connection.socket.send(JSON.stringify(message));
    return true;
  }
}

module.exports = { DeviceGateway };
