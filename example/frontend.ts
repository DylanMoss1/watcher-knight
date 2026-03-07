interface UserData {
  name: string;
  email: string;
  age: number;
}

// WATCHER-KNIGHT FORMAT:
// <wk: <watcher-name> [<files-to-watch (relative to current dir)>]
// Properties to check for & validate />

// <wk: front-and-backend-api-align [./frontend.ts, ./backend.py]
// Ensure that the backend (backend.py) and frontend (frontend.ts) API definitions align />
//
// ^ This will fail: the API definitions do not align
// (The previous result will be cached unless ./frontend.ts or ./backend.py are updated)
class BackendAPI {
  // <wk: only-port-5000
  // Check that this is the only service started on port 5000. />
  //
  // ^ This will pass: this is the only service on port 5000
  constructor(private baseUrl = "http://localhost:5000") { }

  // <wk: error-400-is-handled
  // In app.ts, ensure that error code 400 is handled for this function. />
  //
  // ^ This will fail: the check cannot be completed as app.ts does not exist
  async getUserData(name: string): Promise<UserData> {
    const res = await fetch(
      `${this.baseUrl}/get_user_data?name=${encodeURIComponent(name)}`,
    );
    if (!res.ok) {
      throw new Error(`Request failed: ${res.status} ${res.statusText}`);
    }
    return res.json();
  }
}
