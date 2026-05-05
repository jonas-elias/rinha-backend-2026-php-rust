<?php
declare(strict_types=1);

$sock = getenv('SOCK') ?: '/run/sock/api1.sock';

if (file_exists($sock)) {
    @unlink($sock);
}

$server = new Swoole\Http\Server($sock, 0, SWOOLE_BASE, SWOOLE_SOCK_UNIX_STREAM);

$server->set([
    'worker_num'        => 1,
    'reactor_num'       => 1,
    'tcp_fastopen'      => true,
    'open_tcp_nodelay'  => true,
    'enable_coroutine'  => false,
    'log_level'         => SWOOLE_LOG_WARNING,
    'http_compression'  => false,
    'send_yield'        => false,
    'open_http2_protocol' => false,
    'max_request'       => 0,
    'max_conn'          => 4096,
    'buffer_output_size' => 2 * 1024 * 1024,
]);

// Pre-built static response bodies (mirror Rust constants in src/response.rs).
$BODIES = [
    '{"approved":true,"fraud_score":0.0}',
    '{"approved":true,"fraud_score":0.2}',
    '{"approved":true,"fraud_score":0.4}',
    '{"approved":false,"fraud_score":0.6}',
    '{"approved":false,"fraud_score":0.8}',
    '{"approved":false,"fraud_score":1.0}',
];

$server->on('start', function () use ($sock) {
    @chmod($sock, 0666);
    fwrite(STDERR, "[server] listening on $sock\n");
});

$server->on('request', function (Swoole\Http\Request $req, Swoole\Http\Response $resp) use ($BODIES) {
    $uri = $req->server['request_uri'] ?? '';
    $method = $req->server['request_method'] ?? '';

    if ($uri === '/fraud-score' && $method === 'POST') {
        $idx = rinha_fraud_score($req->rawContent() ?: '{}');
        if ($idx > 5) { $idx = 5; }
        $resp->header('Content-Type', 'application/json', false);
        $resp->end($BODIES[$idx]);
        return;
    }

    if ($uri === '/ready' && $method === 'GET') {
        $resp->status(200);
        $resp->end('');
        return;
    }

    $resp->status(404);
    $resp->end('');
});

$server->start();
