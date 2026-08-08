from http.server import BaseHTTPRequestHandler, HTTPServer
import json

class RequestHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        content_len = int(self.headers.get('content-length', 0))
        post_body = self.rfile.read(content_len).decode('utf-8')
        print(f"Received ticket data: {post_body[:100]}...")
        
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        response = {"token": "MOCK_DENUVO_TOKEN_123"}
        self.wfile.write(json.dumps(response).encode('utf-8'))

if __name__ == '__main__':
    server = HTTPServer(('127.0.0.1', 3030), RequestHandler)
    print('Starting mock token server on port 3030...')
    server.serve_forever()
