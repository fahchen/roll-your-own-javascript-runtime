export namespace JetRuntime {
  export interface Request {
    to: string;
  }

  export interface Context {
    current_user: {
      name: string;
    };
  }

  export class Response {
    status: number;
    data: string;

    constructor(status: number, data: string) {
      this.status = status;
      this.data = data;
    }
  }
}
