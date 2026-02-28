interface UserData {
  name: string;
  email: string;
  age: number;
}

// <watcher-knight: front-and-backend-api-align
//
// files = { ./frontend.ts, ./backend.py }
//
// Ensure that the API definition in backend.py aligns with this API class.
//
// />
class BackendAPI {
  // <watcher-knight: only-port-5000
  //
  // Ensure that this is the only service started on port 5000.
  //
  // />
  constructor(private baseUrl = "http://localhost:5000") { }

  // <watcher-knight: error-400-is-handled
  //
  // In app.ts, ensure that error code 400 is handled for this function.
  //
  // />
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
