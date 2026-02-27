from flask import Flask, jsonify, request
from flask_cors import CORS

app = Flask(__name__)
CORS(app)


@app.route("/get_user_data")
def get_user_data():
    name = request.args.get("name", "Unknown")
    return jsonify(
        {"name": name, "email": f"{name.lower()}@example.com", "age": 30}
    )


if __name__ == "__main__":
    app.run(debug=True)
