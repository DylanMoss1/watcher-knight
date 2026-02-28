interface UserData {
  name: string;
  email: string;
  age: number;
}

// <wk: front-and-backend-api-align [./frontend.ts, ./backend.ts]
// Ensure that the API definition in backend.py aligns with this API class. />
class BackendAPI {
  // <wk: only-port-5000
  // Ensure that this is the only service started on port 5000. />
  constructor(private baseUrl = "http://localhost:5000") { }

  // <wk: error-400-is-handled
  // In app.ts, ensure that error code 400 is handled for this function. />
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
