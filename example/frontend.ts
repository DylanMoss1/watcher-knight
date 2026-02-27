interface UserData {
  name: string;
  email: string;
  age: number;
}

class BackendAPI {
  // <watcher-knight
  //
  // Ensure that the API definition in backend.py aligns with this API class.
  //
  // />

  constructor(private baseUrl = "http://localhost:5000") { }

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
