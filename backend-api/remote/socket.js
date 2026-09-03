const { WebSocketServer } = require('ws');

function attachDeviceSocketServer(server, { service, gateway }) {
  const websocketServer = new WebSocketServer({ noServer: true, maxPayload: 128 * 1024 });

  server.on('upgrade', (request, socket, head) => {
    let url;
    try {
      url = new URL(request.url, 'http://localhost');
    } catch {
      socket.destroy();
      return;
    }
    const match = /^\/api\/([^/]+)\/devices\/connect$/.exec(url.pathname);
    if (!match) return;
    websocketServer.handleUpgrade(request, socket, head, (ws) => {
      websocketServer.emit('connection', ws, request, decodeURIComponent(match[1]));
    });
  });

  websocketServer.on('connection', (socket, request, tenantId) => {
    let connection = null;
    let device = null;
    const authTimer = setTimeout(() => socket.close(4000, 'Authentication timeout'), 5_000);

    socket.on('message', async (buffer) => {
      let message;
      try {
        message = JSON.parse(buffer.toString('utf8'));
      } catch {
        socket.close(4002, 'Invalid message');
        return;
      }
      try {
        if (!connection) {
          if (message.type !== 'hello') throw new Error('HELLO_REQUIRED');
          device = await service.authenticateDevice(tenantId, message.deviceId, message.credential);
          if (!device) {
            socket.close(4003, 'Device authorization failed');
            return;
          }
          clearTimeout(authTimer);
          connection = gateway.attach(device.accountKey, device.deviceId, socket);
          gateway.touch(connection);
          await service.updateDeviceState(tenantId, device, message.state || {});
          socket.send(JSON.stringify({ type: 'hello.accepted', serverTime: new Date().toISOString() }));
          const recoverableJobs = await service.listRecoverableJobsForDevice(
            tenantId,
            device.accountKey,
            device.deviceId
          );
          for (const job of recoverableJobs) {
            socket.send(JSON.stringify({ type: 'remoteJob.dispatch', resume: true, job }));
          }
          return;
        }
        gateway.touch(connection);
        if (message.type === 'heartbeat') {
          if (message.state) await service.updateDeviceState(tenantId, device, message.state);
          socket.send(JSON.stringify({ type: 'heartbeat.ack', serverTime: new Date().toISOString() }));
        } else if (message.type === 'remoteJob.ack') {
          gateway.acknowledge(
            connection.accountKey,
            connection.deviceId,
            String(message.jobId || ''),
            String(message.dispatchNonce || ''),
            message.accepted,
            message.detail
          );
        } else {
          await service.updateJobFromDevice(tenantId, device, message);
        }
      } catch (error) {
        socket.send(JSON.stringify({ type: 'error', code: error.message || 'MESSAGE_REJECTED' }));
      }
    });
    socket.on('close', () => {
      clearTimeout(authTimer);
      if (connection) gateway.detach(connection);
    });
    socket.on('error', () => {
      if (connection) gateway.detach(connection);
    });
  });

  return websocketServer;
}

module.exports = { attachDeviceSocketServer };
