def login(client, password="admin-password"):
    return client.post("/api/auth/login", json={"username": "admin", "password": password})


def test_login_success_sets_session(client):
    assert login(client).status_code == 204
    r = client.get("/api/auth/me")
    assert r.status_code == 200
    assert r.json() == {"username": "admin"}


def test_login_wrong_password_401(client):
    assert login(client, "nope").status_code == 401


def test_protected_route_401_without_auth(client):
    assert client.get("/api/auth/me").status_code == 401


def test_api_key_header_authorizes(client, settings):
    assert client.get("/api/auth/me", headers={"X-Api-Key": settings.api_key}).status_code == 200
    assert client.get("/api/auth/me", headers={"X-Api-Key": "wrong"}).status_code == 401


def test_logout_clears_session(client):
    login(client)
    assert client.post("/api/auth/logout").status_code == 204
    assert client.get("/api/auth/me").status_code == 401


def test_login_rate_limited_after_5_failures(client):
    for _ in range(5):
        assert login(client, "nope").status_code == 401
    assert login(client).status_code == 429


def test_login_non_ascii_password_is_401_not_500(client):
    assert login(client, "mot-de-passe-é").status_code == 401
