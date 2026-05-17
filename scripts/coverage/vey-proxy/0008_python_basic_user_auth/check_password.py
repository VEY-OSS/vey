def check_password(username, password):
    users = {
        "t1": "dogood",
        "t2": "dogood",
    }
    expected = users.get(username)
    if expected is None:
        return False
    return expected == password
