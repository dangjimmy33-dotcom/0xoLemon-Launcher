function FindProxyForURL(url, host) {
    if (shExpMatch(host, "*.steampowered.com") || 
        shExpMatch(host, "*.steamcommunity.com") ||
        shExpMatch(host, "*.steamstatic.com") ||
        shExpMatch(host, "*.steamgames.com")) {
        return "SOCKS5 127.0.0.1:1080; SOCKS 127.0.0.1:1080; DIRECT";
    }
    return "DIRECT";
}