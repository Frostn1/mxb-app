import PropTypes from "prop-types";
import "./Dashboard.scss";
import LoginPage from "../LoginPage/LoginPage";

const Dashboard = ({ isConfigured }) => {
  return (
    <div id={"dashboard"}>
      {!isConfigured ? <LoginPage /> : <div>Hello from configured</div>}
    </div>
  );
};

Dashboard.propTypes = {
  isConfigured: PropTypes.bool.isRequired,
};

export default Dashboard;
