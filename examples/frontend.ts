interface UserData {
  name: string;
  email: string;
  age: number;
}

// WATCHER-KNIGHT FORMAT:
// <wk: <watcher-name> [<files-to-watch (relative to current dir)>]
// options={...}  <-- optional
// Code properties to validate />

// -- EXAMPLE 1: Validating APIs --
// <wk: front-and-backend-api-align [./frontend.ts, ./backend.py]
// Ensure that the backend (backend.py) and frontend (frontend.ts) API definitions align />
//
// ^ FAIL: The API definitions do not align
// (The previous result will be cached unless ./frontend.ts or ./backend.py are updated)
class BackendAPI {
  // -- EXAMPLE 2: Verifying port constraints --
  // <wk: only-port-5000 [./**/*]  <-- recursive on all files in current dir
  // options={model="haiku"}       <-- watcher-specific Claude model
  // Check that this is the only service started on port 5000. />
  //
  // ^ PASS: This is the only service on port 5000
  constructor(private baseUrl = "http://localhost:5000") { }

  // -- EXAMPLE 3: Updating README --
  // <wk: error-400-in-readme  <-- no files specified: watch all files
  // `examples/README.md` should explain what happens when error code 400 is returned. />
  //
  // ^ FAIL: The check cannot be completed as examples/README.md does not exist
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
